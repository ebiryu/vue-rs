//! Lowers the template AST into `El`-builder Rust code.

use std::cell::RefCell;
use std::collections::HashMap;

use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::Ident;

use crate::parser::{Attr, Element, Node};

/// Code generator. When `scope` is set (a `<style scoped>` is present), every
/// element gets a `data-v-<scope>` marker attribute so scoped CSS can target it.
///
/// `slot_payloads` maps each scoped slot's name to its payload type (read from
/// the component's declared `NameSlots` fields), so a `<slot :field="x">` can
/// build the named payload struct it hands to the matching `__slots.<name>`
/// slot builder. `slots` records every `<slot>` the template emits (name and
/// whether it is scoped), so the caller can generate the component's `NameSlots`
/// struct.
pub(crate) struct Codegen {
    scope: Option<String>,
    slot_payloads: HashMap<String, TokenStream>,
    slots: RefCell<Vec<(String, bool)>>,
}

impl Codegen {
    pub(crate) fn new(scope: Option<String>) -> Self {
        Self::with_slot_payloads(scope, HashMap::new())
    }

    pub(crate) fn with_slot_payloads(
        scope: Option<String>,
        slot_payloads: HashMap<String, TokenStream>,
    ) -> Self {
        Self {
            scope,
            slot_payloads,
            slots: RefCell::new(Vec::new()),
        }
    }

    /// Every `<slot>` the template emitted, as `(name, scoped)` pairs (deduped by
    /// name, in first-seen order). Meaningful after [`root`](Self::root) has run;
    /// the caller uses it to generate the component's `NameSlots` struct.
    pub(crate) fn slots(&self) -> Vec<(String, bool)> {
        self.slots.borrow().clone()
    }

    /// Record a `<slot>` usage, ignoring a repeat of a name already seen.
    fn record_slot(&self, name: &str, scoped: bool) {
        let mut slots = self.slots.borrow_mut();
        if !slots.iter().any(|(n, _)| n == name) {
            slots.push((name.to_string(), scoped));
        }
    }

    /// Generate the builder expression for a template's root. A single root
    /// element lowers directly to that element; multiple roots (or root-level
    /// text) lower to a fragment that mounts, moves, and unmounts as a unit.
    pub(crate) fn root(&self, nodes: &[Node]) -> Result<TokenStream, String> {
        match nodes {
            [] => Err("template must not be empty".to_string()),
            [Node::Element(only)] => self.element(only),
            _ => {
                let members = nodes
                    .iter()
                    .map(|node| self.root_member(node))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(quote! {
                    ::vue_rs_dom::Backend::create_fragment(
                        &__backend,
                        ::std::vec![ #(#members),* ],
                    )
                })
            }
        }
    }

    /// Build one member of a multi-root template as a `B::Node`. Elements and
    /// components lower as usual; text becomes a (possibly reactive) text node.
    /// Root-level control flow has no enclosing element to anchor against, so it
    /// is rejected with a clear message.
    fn root_member(&self, node: &Node) -> Result<TokenStream, String> {
        match node {
            Node::StaticText(text) => {
                Ok(quote! { ::vue_rs_dom::Backend::create_text(&__backend, #text) })
            }
            Node::DynText(expr) => {
                let expr = parse_expr(expr)?;
                Ok(quote! {
                    ::vue_rs_dom::dyn_text_node(&__backend, move || (#expr).to_string())
                })
            }
            Node::Element(el) => {
                for directive in ["v-if", "v-else-if", "v-else", "v-for"] {
                    if find_static(el, directive).is_some() {
                        return Err(format!(
                            "`{directive}` is not supported at the template root; wrap it in an element"
                        ));
                    }
                }
                self.element(el)
            }
        }
    }

    fn element(&self, el: &Element) -> Result<TokenStream, String> {
        if el.tag == "slot" {
            let name = find_static(el, "name").unwrap_or("default");
            let dyn_attrs: Vec<(&str, &str)> = el
                .attrs
                .iter()
                .filter_map(|attr| match attr {
                    Attr::Dyn { name, expr } => Some((name.as_str(), expr.as_str())),
                    _ => None,
                })
                .collect();
            let fallback = self.slot_fallback(el)?;
            let field = Ident::new(name, Span::call_site());
            // Every slot is held as an `Option<SlotFn<_, _>>` field on the
            // component's `__slots` struct. A plain slot carries a `()` payload; a
            // scoped slot builds its declared payload struct from the `:field="x"`
            // attributes. Either way, an unprovided slot renders the fallback.
            let payload = if dyn_attrs.is_empty() {
                self.record_slot(name, false);
                quote! { () }
            } else {
                self.record_slot(name, true);
                let payload_ty = self.slot_payloads.get(name).ok_or_else(|| {
                    format!(
                        "scoped slot `{name}` needs a `{name}: _` field in the component's Slots struct"
                    )
                })?;
                let mut payload_fields = Vec::new();
                for (attr, expr) in dyn_attrs {
                    if dynamic_arg(attr).is_some() {
                        return Err(format!(
                            "dynamic arguments (`{attr}`) are not supported on `<slot>`"
                        ));
                    }
                    let attr = Ident::new(attr, Span::call_site());
                    let expr = parse_expr(expr)?;
                    payload_fields.push(quote! { #attr: #expr });
                }
                quote! { #payload_ty { #(#payload_fields),* } }
            };
            return Ok(quote! {
                match &__slots.#field {
                    ::core::option::Option::Some(__slot) => {
                        __slot.render(__backend.clone(), #payload)
                    }
                    ::core::option::Option::None => #fallback,
                }
            });
        }
        if is_component(&el.tag) {
            return self.component(el);
        }
        let tag = &el.tag;
        let mut chain = quote! { El::new(__backend.clone(), #tag) };
        if let Some(scope) = &self.scope {
            let marker = format!("data-v-{scope}");
            chain = quote! { #chain.attr(#marker, "") };
        }
        // When `v-show` shares an element with the element's own `style` (static
        // `style="…"` or dynamic `:style="…"`), `v-show` folds the `display: none`
        // toggle into that base style instead of emitting its own `style`
        // attribute (which would clobber the other one). `base_style` is the
        // expression that yields the base style `String`; the folded `style` attr
        // is then skipped below since it is already woven in.
        let base_style: Option<TokenStream> = if find_static(el, "v-show").is_some() {
            Some(
                style_value(find_static(el, "style"), find_dyn(el, "style"))?
                    .unwrap_or_else(|| quote! { ::std::string::String::new() }),
            )
        } else {
            None
        };
        // A static `class` and a dynamic `:class` (object/array/plain) are merged
        // into one `class` string via `ClassList`. The merged attribute is emitted
        // once, at the first class attribute's position; later class attributes are
        // skipped. A lone plain `:class` (no static sibling) keeps the simpler
        // `gen_attr` path below.
        let merged_class: Option<TokenStream> = match find_dyn(el, "class") {
            Some(expr)
                if find_static(el, "class").is_some()
                    || !matches!(class_syntax(expr), ClassSyntax::Plain) =>
            {
                Some(gen_class(find_static(el, "class"), expr)?)
            }
            _ => None,
        };
        // A static `style` and a dynamic `:style` (object/array/plain) are merged
        // into one `style` string via `StyleList`, mirroring `:class`. When
        // `v-show` is present it already owns the `style` attribute (folding the
        // toggle into `base_style`), so there is no separate merge then.
        let merged_style: Option<TokenStream> = if base_style.is_some() {
            None
        } else {
            match find_dyn(el, "style") {
                Some(expr)
                    if find_static(el, "style").is_some()
                        || !matches!(style_syntax(expr), StyleSyntax::Plain) =>
                {
                    style_value(find_static(el, "style"), Some(expr))?
                        .map(|value| quote! { .dyn_attr("style", move || #value) })
                }
                _ => None,
            }
        };
        // `v-model` lowers differently per input kind. A static `type` (the only
        // form we can resolve at compile time) tells a checkbox apart from a text
        // input.
        let input_type = if el.tag == "input" {
            find_static(el, "type")
        } else {
            None
        };
        // A radio's `v-model` maps the model against the element's `value`
        // (static `value="x"` or dynamic `:value="expr"`); capture it here so
        // `gen_attr` can build the `checked`/`change` binding.
        let radio_value = if input_type == Some("radio") {
            match (find_static(el, "value"), find_dyn(el, "value")) {
                (Some(s), _) => Some(RadioValue::Static(s.to_string())),
                (None, Some(expr)) => Some(RadioValue::Dyn(parse_expr(expr)?)),
                (None, None) => None,
            }
        } else {
            None
        };
        let mut class_emitted = false;
        let mut style_emitted = false;
        for attr in &el.attrs {
            if is_structural(attr) {
                continue; // v-if / v-else-if / v-else / v-for / :key are handled by the parent
            }
            if base_style.is_some() && is_style_attr(attr) {
                continue; // folded into the `v-show` toggle below
            }
            if let Some(merged) = &merged_style
                && is_style_attr(attr)
            {
                if !style_emitted {
                    chain = quote! { #chain #merged };
                    style_emitted = true;
                }
                continue;
            }
            if let Some(merged) = &merged_class
                && is_class_attr(attr)
            {
                if !class_emitted {
                    chain = quote! { #chain #merged };
                    class_emitted = true;
                }
                continue;
            }
            let part = gen_attr(attr, base_style.as_ref(), &el.tag, input_type, radio_value.as_ref())?;
            chain = quote! { #chain #part };
        }
        // `v-html` and `v-text` own the element's content, so any template
        // children are ignored (matching Vue).
        if find_static(el, "v-html").is_none() && find_static(el, "v-text").is_none() {
            for part in self.children(&el.children)? {
                chain = quote! { #chain #part };
            }
        }
        Ok(quote! { #chain.finish() })
    }

    /// A PascalCase tag is a component: emit `Tag(__backend.clone(), TagProps { .. })`.
    /// `:name` becomes a prop field, `@name` becomes `on_name: Callback::new(..)`.
    fn component(&self, el: &Element) -> Result<TokenStream, String> {
        let component = Ident::new(&el.tag, Span::call_site());
        let mut fields = Vec::new();
        for attr in &el.attrs {
            if is_structural(attr) {
                continue;
            }
            match attr {
                Attr::Dyn { name, .. } | Attr::Event { name, .. }
                    if dynamic_arg(name).is_some() =>
                {
                    return Err(format!(
                        "dynamic arguments (`{name}`) are not supported on components"
                    ));
                }
                Attr::Dyn { name, expr } => {
                    let field = Ident::new(name, Span::call_site());
                    let expr = parse_expr(expr)?;
                    // Reactive props flow down read-only: `Into` converts a
                    // `Signal`/`Memo` to the child's `ReadSignal` field (and is
                    // the identity for plain values, whose target type is fixed
                    // by the struct field).
                    fields.push(quote! { #field: ::core::convert::Into::into(#expr) });
                }
                Attr::Event { name, handler } => {
                    let field = Ident::new(&format!("on_{name}"), Span::call_site());
                    let handler = parse_expr(handler)?;
                    fields.push(quote! { #field: ::vue_rs_dom::Callback::new(#handler) });
                }
                Attr::Static { name, value } if is_component_v_model(name) => {
                    // A component `v-model[:arg]` lowers to prop-down / emit-up:
                    // the value flows down read-only under `model_<arg>` (default
                    // arg `value`), and updates flow up through an
                    // `on_update_model_<arg>` callback that writes the source. The
                    // source expression must therefore be a writable handle
                    // (`Signal`/`WritableMemo`), like an element `v-model`.
                    let (value_field, update_field) = component_model_fields(name)?;
                    let expr = parse_expr(value)?;
                    fields.push(quote! { #value_field: ::core::convert::Into::into(#expr) });
                    fields.push(quote! {
                        #update_field: ::vue_rs_dom::Callback::new(move |__v| (#expr).set(__v))
                    });
                }
                Attr::Static { name, .. } if name == "ref" => {
                    return Err(
                        "template refs on components (the component instance) are not supported"
                            .to_string(),
                    );
                }
                Attr::Static { name, value } => {
                    let field = Ident::new(name, Span::call_site());
                    fields.push(quote! { #field: #value });
                }
            }
        }
        // Every child this parent provides is funnelled into the component's
        // generated `NameSlots` struct, built up from `for_backend` (which pins
        // the backend type) via per-slot `with_<name>` builders. Each builder
        // names its slot's payload type, so a scoped binding `pat` needs no
        // annotation. Unprovided slots keep their `None` default, so the parent
        // may provide any subset — including nothing at all. The struct is always
        // passed, so a slot-bearing component works even with no slots provided.
        let mut setters: Vec<TokenStream> = Vec::new();
        let mut default_content: Vec<&Element> = Vec::new();
        for child in &el.children {
            let Node::Element(child_el) = child else {
                continue;
            };
            match slot_directive(child_el) {
                Some((name, binding)) if child_el.tag == "template" => {
                    let content = self.single_root(&child_el.children)?;
                    let setter = Ident::new(&format!("with_{name}"), Span::call_site());
                    if binding.is_empty() {
                        // A plain named slot: no payload, bound as `()`.
                        setters.push(quote! { .#setter(move |__backend, ()| #content) });
                    } else {
                        let pat: TokenStream = binding
                            .parse()
                            .map_err(|e| format!("invalid v-slot binding `{binding}`: {e}"))?;
                        setters.push(quote! { .#setter(move |__backend, #pat| #content) });
                    }
                }
                _ => default_content.push(child_el),
            }
        }
        match default_content.as_slice() {
            [] => {}
            [only] => {
                let content = self.element(only)?;
                setters.push(quote! { .with_default(move |__backend, ()| #content) });
            }
            _ => return Err("default slot content must be a single root element".to_string()),
        }

        let mut args = vec![quote! { __backend.clone() }];
        if !fields.is_empty() {
            let props = Ident::new(&format!("{}Props", el.tag), Span::call_site());
            args.push(quote! { #props { #(#fields),* } });
        }
        let slots = Ident::new(&format!("{}Slots", el.tag), Span::call_site());
        args.push(quote! { #slots::for_backend(&__backend) #(#setters)* });
        Ok(quote! { #component(#(#args),*) })
    }

    /// The fallback a `<slot>` renders when the parent provided nothing: its own
    /// child element, or an empty anchor if it has none.
    fn slot_fallback(&self, el: &Element) -> Result<TokenStream, String> {
        let elements: Vec<&Element> = el
            .children
            .iter()
            .filter_map(|n| match n {
                Node::Element(e) => Some(e),
                _ => None,
            })
            .collect();
        match elements.as_slice() {
            [] => Ok(quote! { ::vue_rs_dom::Backend::create_anchor(&__backend) }),
            [only] => self.element(only),
            _ => Err("slot fallback content must be a single root element".to_string()),
        }
    }

    /// The single root element among `children` (used for slot template content).
    fn single_root(&self, children: &[Node]) -> Result<TokenStream, String> {
        let elements: Vec<&Element> = children
            .iter()
            .filter_map(|n| match n {
                Node::Element(e) => Some(e),
                _ => None,
            })
            .collect();
        match elements.as_slice() {
            [only] => self.element(only),
            _ => Err("slot content must be a single root element".to_string()),
        }
    }

    fn children(&self, children: &[Node]) -> Result<Vec<TokenStream>, String> {
        let mut parts = Vec::new();
        let mut i = 0;
        while i < children.len() {
            match &children[i] {
                Node::StaticText(text) => parts.push(quote! { .text(#text) }),
                Node::DynText(expr) => {
                    let expr = parse_expr(expr)?;
                    parts.push(quote! { .dyn_text(move || (#expr).to_string()) });
                }
                Node::Element(el) => {
                    if let Some(for_expr) = find_static(el, "v-for") {
                        parts.push(self.gen_for(el, for_expr)?);
                    } else if let Some(cond) = find_static(el, "v-if") {
                        // Collect the `v-if` / `v-else-if`* / `v-else`? chain: each
                        // arm is a `(condition, element)`, with `None` condition for
                        // the terminal `v-else`. Whitespace-only text between arms is
                        // dropped by the parser, so siblings are directly adjacent.
                        let mut arms: Vec<(Option<&str>, &Element)> = vec![(Some(cond), el)];
                        let mut j = i + 1;
                        while let Some(Node::Element(next)) = children.get(j) {
                            if let Some(cond) = find_static(next, "v-else-if") {
                                arms.push((Some(cond), next));
                                j += 1;
                            } else if find_static(next, "v-else").is_some() {
                                arms.push((None, next));
                                j += 1;
                                break;
                            } else {
                                break;
                            }
                        }
                        parts.push(self.gen_if_chain(&arms)?);
                        i = j;
                        continue;
                    } else if find_static(el, "v-else-if").is_some() {
                        return Err("v-else-if without a matching v-if".to_string());
                    } else if find_static(el, "v-else").is_some() {
                        return Err("v-else without a matching v-if".to_string());
                    } else {
                        let child = self.element(el)?;
                        parts.push(quote! { .child(#child) });
                    }
                }
            }
            i += 1;
        }
        Ok(parts)
    }

    /// Lower a `v-if` / `v-else-if`* / `v-else`? chain (`arms` as
    /// `(condition, element)`, with `None` condition for the terminal `v-else`)
    /// to a `dyn_switch`. The selector returns the index of the first arm whose
    /// condition holds — the `v-else` arm always holds — or `None` when none do.
    fn gen_if_chain(&self, arms: &[(Option<&str>, &Element)]) -> Result<TokenStream, String> {
        let mut selector_arms = Vec::new();
        let mut pushes = Vec::new();
        // The terminal `v-else` arm (a `None` condition) always matches, so it
        // becomes the selector's tail expression. Without one, the tail is `None`.
        let mut tail = quote! { ::core::option::Option::None };
        for (i, (cond, el)) in arms.iter().enumerate() {
            let view = self.element(el)?;
            pushes.push(quote! {
                __views.push(::std::boxed::Box::new(move |__backend| #view));
            });
            match cond {
                Some(cond) => {
                    let cond = parse_expr(cond)?;
                    selector_arms.push(quote! {
                        if (#cond) { return ::core::option::Option::Some(#i); }
                    });
                }
                None => tail = quote! { ::core::option::Option::Some(#i) },
            }
        }
        Ok(quote! {
            .dyn_switch(
                move || {
                    #(#selector_arms)*
                    #tail
                },
                {
                    let mut __views = ::vue_rs_dom::switch_views(&__backend);
                    #(#pushes)*
                    __views
                },
            )
        })
    }

    fn gen_for(&self, el: &Element, for_expr: &str) -> Result<TokenStream, String> {
        let (binding, iterable) = for_expr
            .split_once(" in ")
            .ok_or_else(|| format!("v-for must be `item in items`, got `{for_expr}`"))?;
        let binding = binding.trim();
        let iterable = parse_expr(iterable.trim())?;
        let key = find_dyn(el, "key").ok_or("v-for requires a :key binding")?;
        let key = parse_expr(key)?;
        let view = self.element(el)?;

        // `(item, index)` binds the position alongside the item. We enumerate the
        // iterable into `(usize, T)` rows and destructure with the index first to
        // match `enumerate`'s order. The index is captured at build time, so a
        // reused row keeps its original index when the list is reordered.
        if let Some((item_pat, index_pat)) = parse_index_binding(binding)? {
            return Ok(quote! {
                .dyn_for(
                    move || (#iterable).into_iter().enumerate().collect::<::std::vec::Vec<_>>(),
                    |(#index_pat, #item_pat)| (#key).clone(),
                    move |__backend, (#index_pat, #item_pat)| #view,
                )
            });
        }

        let binding: TokenStream = binding
            .parse()
            .map_err(|e| format!("invalid v-for binding: {e}"))?;
        Ok(quote! {
            .dyn_for(
                move || (#iterable),
                |#binding| (#key).clone(),
                move |__backend, #binding| #view,
            )
        })
    }
}

/// Parse a `v-for` binding's `(item, index)` tuple form, returning the item and
/// index patterns. Returns `Ok(None)` for the single-identifier form `item`.
fn parse_index_binding(binding: &str) -> Result<Option<(TokenStream, TokenStream)>, String> {
    let Some(inner) = binding.strip_prefix('(').and_then(|s| s.strip_suffix(')')) else {
        return Ok(None);
    };
    let (item, index) = inner
        .split_once(',')
        .ok_or_else(|| format!("v-for tuple binding must be `(item, index)`, got `{binding}`"))?;
    let item: TokenStream = item
        .trim()
        .parse()
        .map_err(|e| format!("invalid v-for item binding: {e}"))?;
    let index: TokenStream = index
        .trim()
        .parse()
        .map_err(|e| format!("invalid v-for index binding: {e}"))?;
    Ok(Some((item, index)))
}

fn is_component(tag: &str) -> bool {
    tag.chars().next().is_some_and(|c| c.is_ascii_uppercase())
}

/// Whether an attribute name is a component `v-model` (bare, `:arg`, or with
/// modifiers).
fn is_component_v_model(name: &str) -> bool {
    name == "v-model" || name.starts_with("v-model:") || name.starts_with("v-model.")
}

/// The prop and emit fields a component `v-model[:arg]` binds: the value prop
/// (`model_value` by default, `model_<arg>` for `v-model:arg`) and the matching
/// `on_update_<...>` callback. Modifiers are rejected (not yet supported).
fn component_model_fields(name: &str) -> Result<(Ident, Ident), String> {
    let rest = name.strip_prefix("v-model").expect("checked by caller");
    // Split off any `.modifiers`; the leading part is `""` or `:arg`.
    let (arg_part, modifiers) = match rest.split_once('.') {
        Some((arg, mods)) => (arg, Some(mods)),
        None => (rest, None),
    };
    if modifiers.is_some() {
        return Err(format!(
            "modifiers on a component `v-model` are not supported (`{name}`)"
        ));
    }
    // Default (no arg) binds the conventional `model_value` prop; `v-model:arg`
    // binds `arg` directly (Vue's `modelValue` vs. named-model naming). The emit
    // is `on_update_<prop>` either way.
    let prop = match arg_part.strip_prefix(':') {
        None => "model_value".to_string(),
        Some("") => return Err("a component `v-model:` argument must not be empty".to_string()),
        Some(arg) => arg.to_string(),
    };
    let value_field = Ident::new(&prop, Span::call_site());
    let update_field = Ident::new(&format!("on_update_{prop}"), Span::call_site());
    Ok((value_field, update_field))
}

/// The slot name and binding pattern from a `<template v-slot:name="pat">`
/// element, if present. The binding is empty for a plain `<template v-slot:name>`.
fn slot_directive(el: &Element) -> Option<(&str, &str)> {
    el.attrs.iter().find_map(|attr| match attr {
        Attr::Static { name, value } => name.strip_prefix("v-slot:").map(|n| (n, value.as_str())),
        _ => None,
    })
}

/// The `value` a radio `<input>` carries, used to build its `v-model` binding:
/// a static `value="x"` (compared/written as a string) or a dynamic `:value`
/// expression (compared/written by value).
enum RadioValue {
    Static(String),
    Dyn(syn::Expr),
}

fn gen_attr(
    attr: &Attr,
    base_style: Option<&TokenStream>,
    tag: &str,
    input_type: Option<&str>,
    radio_value: Option<&RadioValue>,
) -> Result<TokenStream, String> {
    match attr {
        Attr::Static { name, value } if name == "v-model" || name.starts_with("v-model.") => {
            let model = parse_expr(value)?;
            // On `<input type="checkbox">`, `v-model` binds the boolean `checked`
            // property: the model drives `checked`, and a `change` carries the new
            // state back (`"true"`/`"false"`). The text modifiers do not apply.
            if input_type == Some("checkbox") {
                if let Some(modifier) = name.split('.').nth(1) {
                    return Err(format!(
                        "v-model modifier `.{modifier}` is not supported on a checkbox"
                    ));
                }
                return Ok(quote! {
                    .dyn_bool_prop("checked", move || (#model).get())
                    .on_value("change", move |__value| (#model).set(__value == "true"))
                });
            }
            // On `<input type="radio">`, `v-model` maps the model against the
            // radio's own `value`: `checked` reflects equality and selecting it
            // writes that value back. The `value` (static or `:value`) is still
            // rendered as a normal attribute. The text modifiers do not apply.
            if input_type == Some("radio") {
                if let Some(modifier) = name.split('.').nth(1) {
                    return Err(format!(
                        "v-model modifier `.{modifier}` is not supported on a radio"
                    ));
                }
                let value = radio_value.ok_or_else(|| {
                    "`v-model` on a radio input requires a `value` or `:value` attribute"
                        .to_string()
                })?;
                let (checked, set_val) = match value {
                    RadioValue::Static(s) => {
                        (quote! { (#model).get() == #s }, quote! { #s.to_string() })
                    }
                    RadioValue::Dyn(expr) => {
                        (quote! { (#model).get() == (#expr) }, quote! { #expr })
                    }
                };
                return Ok(quote! {
                    .dyn_bool_prop("checked", move || #checked)
                    .on_value("change", move |_| (#model).set(#set_val))
                });
            }
            // Modifiers refine the binding: `.lazy` syncs on `change` instead of
            // `input`; `.trim` strips surrounding whitespace; `.number` parses the
            // value into the model's type, keeping the current value when the input
            // is not a valid number. `.trim` applies before `.number`.
            let mut lazy = false;
            let mut trim = false;
            let mut number = false;
            for m in name.split('.').skip(1) {
                match m {
                    "lazy" => lazy = true,
                    "trim" => trim = true,
                    "number" => number = true,
                    other => {
                        return Err(format!("unknown v-model modifier `.{other}`"));
                    }
                }
            }
            // `<textarea>` and `<select>` have no `value` content attribute, so
            // their `v-model` drives the `value` DOM property. A `<select>` commits
            // a choice rather than streaming keystrokes, so it syncs on `change`.
            let is_textarea = tag == "textarea";
            let is_select = tag == "select";
            let event = if lazy || is_select { "change" } else { "input" };
            let text = if trim {
                quote! { __value.trim() }
            } else {
                quote! { __value }
            };
            let set_arg = if number {
                quote! {
                    match #text.parse() {
                        ::core::result::Result::Ok(__n) => __n,
                        ::core::result::Result::Err(_) => (#model).get(),
                    }
                }
            } else {
                quote! { #text.to_string() }
            };
            let bind = if is_textarea || is_select {
                quote! { .dyn_prop("value", move || ((#model).get()).to_string()) }
            } else {
                quote! { .dyn_attr("value", move || ((#model).get()).to_string()) }
            };
            Ok(quote! {
                #bind
                .on_value(#event, move |__value| (#model).set(#set_arg))
            })
        }
        // `v-show` keeps the element mounted (unlike `v-if`) and reactively
        // collapses it with inline `display: none` when the expression is falsy.
        // The element's own `style` (static or `:style`, captured as `base_style`)
        // is preserved: the toggle appends `display: none` to it rather than
        // replacing it. The base is re-evaluated inside the effect, so a reactive
        // `:style` stays live.
        Attr::Static { name, value } if name == "v-show" => {
            let cond = parse_expr(value)?;
            let base = match base_style {
                Some(base) => quote! { #base },
                None => quote! { ::std::string::String::new() },
            };
            Ok(quote! {
                .dyn_attr("style", move || {
                    let __base: ::std::string::String = #base;
                    if (#cond) {
                        __base
                    } else if __base.is_empty() {
                        "display: none".to_string()
                    } else {
                        ::std::format!("{}; display: none", __base)
                    }
                })
            })
        }
        // `v-html` sets the element's inner HTML from a (reactive) expression,
        // replacing any children. The expression must yield a `RawHtml`, so the
        // unescaped insertion is an explicit opt-in at the call site.
        Attr::Static { name, value } if name == "v-html" => {
            let expr = parse_expr(value)?;
            Ok(quote! { .dyn_inner_html(move || #expr) })
        }
        // `v-text` is sugar for a single `{{ }}` child: it sets the element's
        // text content (escaped) from a reactive expression, replacing any
        // template children.
        Attr::Static { name, value } if name == "v-text" => {
            let expr = parse_expr(value)?;
            Ok(quote! { .dyn_text(move || (#expr).to_string()) })
        }
        // `ref="name"` binds the element's node into the `name` template ref
        // handle (declared as `let name = template_ref();` in the component's
        // `<script>`). The value names that binding, so it must be an identifier.
        Attr::Static { name, value } if name == "ref" => {
            let binding: Ident = syn::parse_str(value)
                .map_err(|_| format!("`ref` value must be an identifier, got {value:?}"))?;
            Ok(quote! { .node_ref(&#binding) })
        }
        Attr::Static { name, value } => Ok(quote! { .attr(#name, #value) }),
        Attr::Dyn { name, expr } => {
            // A dynamic argument `:[arg]` computes the attribute name at runtime.
            if let Some((arg, modifiers)) = dynamic_arg(name) {
                if !modifiers.is_empty() {
                    return Err(format!(
                        "modifiers are not supported on dynamic attribute arguments (`:{name}`)"
                    ));
                }
                let arg = parse_expr(arg)?;
                let value = parse_expr(expr)?;
                return Ok(quote! {
                    .dyn_attr_named(move || (#arg).to_string(), move || (#value).to_string())
                });
            }
            // `:name.modifiers` refine the binding: `.prop` sets a DOM property
            // instead of an attribute, `.attr` forces an attribute (the default,
            // spelled explicitly), and `.camel` camelizes the bound name.
            let (base, modifiers) = split_modifiers(name);
            let mut prop = false;
            let mut force_attr = false;
            let mut camel = false;
            for m in modifiers {
                match m {
                    "prop" => prop = true,
                    "attr" => force_attr = true,
                    "camel" => camel = true,
                    other => return Err(format!("unknown bind modifier `.{other}` on `:{name}`")),
                }
            }
            if prop && force_attr {
                return Err(format!("`:{name}` cannot be both `.prop` and `.attr`"));
            }
            let bound = if camel { camelize(base) } else { base.to_string() };
            let expr = parse_expr(expr)?;
            if prop {
                Ok(quote! { .dyn_prop(#bound, move || (#expr).to_string()) })
            } else {
                Ok(quote! { .dyn_attr(#bound, move || (#expr).to_string()) })
            }
        }
        Attr::Event { name, handler } => {
            // A dynamic argument `@[arg]` computes the event name at runtime.
            if let Some((arg, modifiers)) = dynamic_arg(name) {
                if !modifiers.is_empty() {
                    return Err(format!(
                        "modifiers are not supported on dynamic event arguments (`@{name}`)"
                    ));
                }
                let arg = parse_expr(arg)?;
                let handler = parse_expr(handler)?;
                return Ok(quote! { .on_named(move || (#arg).to_string(), move || { #handler }) });
            }
            let handler = parse_expr(handler)?;
            let mut parts = name.split('.');
            let event = parts.next().unwrap_or("");
            let modifiers: Vec<&str> = parts.collect();
            if modifiers.is_empty() {
                Ok(quote! { .on(#event, move || { #handler }) })
            } else {
                let opts = build_event_options(event, &modifiers, name)?;
                Ok(quote! { .on_opts(#event, #opts, move || { #handler }) })
            }
        }
    }
}

/// Build the `EventOptions` literal for an `@event`'s modifiers. Only set fields
/// are emitted (the rest fall back via `..Default::default()`). Key vs.
/// mouse-button modifiers are disambiguated by the event name; unknown modifiers
/// are a compile error so typos surface early.
fn build_event_options(
    event: &str,
    modifiers: &[&str],
    full: &str,
) -> Result<TokenStream, String> {
    let mut fields: Vec<TokenStream> = Vec::new();
    let mut keys: Vec<&'static str> = Vec::new();
    let mut buttons: Vec<u16> = Vec::new();
    let keyboard = is_keyboard_event(event);
    let mouse = is_mouse_event(event);
    for &m in modifiers {
        match m {
            "prevent" => fields.push(quote! { prevent_default: true }),
            "stop" => fields.push(quote! { stop_propagation: true }),
            "once" => fields.push(quote! { once: true }),
            "capture" => fields.push(quote! { capture: true }),
            "passive" => fields.push(quote! { passive: true }),
            "self" => fields.push(quote! { self_only: true }),
            "ctrl" => fields.push(quote! { ctrl: true }),
            "alt" => fields.push(quote! { alt: true }),
            "shift" => fields.push(quote! { shift: true }),
            "meta" => fields.push(quote! { meta: true }),
            "exact" => fields.push(quote! { exact: true }),
            other => {
                if mouse && let Some(b) = mouse_button(other) {
                    buttons.push(b);
                } else if keyboard && let Some(k) = key_name(other) {
                    keys.push(k);
                } else {
                    return Err(format!("unknown event modifier `.{other}` on `@{full}`"));
                }
            }
        }
    }
    if !keys.is_empty() {
        fields.push(quote! { keys: &[ #(#keys),* ] });
    }
    if !buttons.is_empty() {
        fields.push(quote! { buttons: &[ #(#buttons),* ] });
    }
    Ok(quote! {
        ::vue_rs_dom::EventOptions { #(#fields,)* ..::core::default::Default::default() }
    })
}

fn is_keyboard_event(event: &str) -> bool {
    matches!(event, "keyup" | "keydown" | "keypress")
}

fn is_mouse_event(event: &str) -> bool {
    matches!(
        event,
        "click"
            | "dblclick"
            | "auxclick"
            | "mousedown"
            | "mouseup"
            | "mousemove"
            | "mouseenter"
            | "mouseleave"
            | "mouseover"
            | "mouseout"
            | "contextmenu"
    )
}

/// Map a mouse-button modifier to its `MouseEvent.button` value.
fn mouse_button(m: &str) -> Option<u16> {
    match m {
        "left" => Some(0),
        "middle" => Some(1),
        "right" => Some(2),
        _ => None,
    }
}

/// Map a key modifier to the matching `KeyboardEvent.key` string.
fn key_name(m: &str) -> Option<&'static str> {
    Some(match m {
        "enter" => "Enter",
        "esc" | "escape" => "Escape",
        "tab" => "Tab",
        "space" => " ",
        "delete" => "Delete",
        "up" => "ArrowUp",
        "down" => "ArrowDown",
        "left" => "ArrowLeft",
        "right" => "ArrowRight",
        "home" => "Home",
        "end" => "End",
        _ => return None,
    })
}

/// The shape of a `:class` binding's expression.
enum ClassSyntax {
    /// `{ name: cond, .. }` — toggle each class by its condition.
    Object,
    /// `[a, b, ..]` — join each element's class string.
    Array,
    /// Any other expression, yielding a class string directly.
    Plain,
}

fn class_syntax(expr: &str) -> ClassSyntax {
    let t = expr.trim();
    if t.starts_with('{') && t.ends_with('}') {
        ClassSyntax::Object
    } else if t.starts_with('[') && t.ends_with(']') {
        ClassSyntax::Array
    } else {
        ClassSyntax::Plain
    }
}

/// Build the merged `class` attribute for an element carrying a dynamic `:class`,
/// folding in the static `class` (if any) as the leading fragment. Object entries
/// become `push_if`, array elements and a plain expression become `push`.
fn gen_class(static_class: Option<&str>, dyn_expr: &str) -> Result<TokenStream, String> {
    let mut chain = quote! { ::vue_rs_dom::ClassList::new() };
    if let Some(value) = static_class {
        chain = quote! { #chain.push(#value) };
    }
    match class_syntax(dyn_expr) {
        ClassSyntax::Object => {
            for (name, cond) in parse_class_object(dyn_expr)? {
                let cond = parse_expr(&cond)?;
                chain = quote! { #chain.push_if(#name, #cond) };
            }
        }
        ClassSyntax::Array => {
            let array: syn::ExprArray = syn::parse_str(dyn_expr)
                .map_err(|e| format!("invalid `:class` array `{dyn_expr}`: {e}"))?;
            for elem in array.elems {
                chain = quote! { #chain.push(#elem) };
            }
        }
        ClassSyntax::Plain => {
            let expr = parse_expr(dyn_expr)?;
            chain = quote! { #chain.push(#expr) };
        }
    }
    Ok(quote! { .dyn_attr("class", move || #chain.finish()) })
}

/// Parse a `:class` object literal `{ name: cond, .. }` into `(name, condition)`
/// pairs. A name may be a bare token (`active`) or a quoted string
/// (`'text-danger'`); either way the class name is taken verbatim.
fn parse_class_object(expr: &str) -> Result<Vec<(String, String)>, String> {
    let inner = expr
        .trim()
        .strip_prefix('{')
        .and_then(|s| s.strip_suffix('}'))
        .ok_or_else(|| format!("invalid `:class` object `{expr}`"))?;
    let mut entries = Vec::new();
    for raw in split_top_level(inner, ',') {
        let entry = raw.trim();
        if entry.is_empty() {
            continue;
        }
        let colon = find_object_colon(entry)
            .ok_or_else(|| format!("`:class` object entry `{entry}` must be `name: condition`"))?;
        let name = unquote(entry[..colon].trim());
        let cond = entry[colon + 1..].trim();
        if cond.is_empty() {
            return Err(format!("`:class` object entry `{entry}` has no condition"));
        }
        entries.push((name, cond.to_string()));
    }
    Ok(entries)
}

/// The shape of a `:style` binding's expression.
enum StyleSyntax {
    /// `{ prop: value, .. }` — one `prop: value` declaration per entry.
    Object,
    /// `[a, b, ..]` — join each element's declaration string.
    Array,
    /// Any other expression, yielding a declaration string directly.
    Plain,
}

fn style_syntax(expr: &str) -> StyleSyntax {
    let t = expr.trim();
    if t.starts_with('{') && t.ends_with('}') {
        StyleSyntax::Object
    } else if t.starts_with('[') && t.ends_with(']') {
        StyleSyntax::Array
    } else {
        StyleSyntax::Plain
    }
}

/// Build the expression that yields an element's merged `style` string from its
/// static `style` and dynamic `:style` (object/array/plain), via `StyleList`.
/// Returns `None` when the element has neither, so the caller can fall back to an
/// empty string (`v-show`) or skip the merge entirely.
fn style_value(
    static_style: Option<&str>,
    dyn_style: Option<&str>,
) -> Result<Option<TokenStream>, String> {
    if static_style.is_none() && dyn_style.is_none() {
        return Ok(None);
    }
    let mut chain = quote! { ::vue_rs_dom::StyleList::new() };
    if let Some(value) = static_style {
        chain = quote! { #chain.push(#value) };
    }
    if let Some(expr) = dyn_style {
        match style_syntax(expr) {
            StyleSyntax::Object => {
                for (prop, value) in parse_style_object(expr)? {
                    let value = parse_expr(&value)?;
                    chain = quote! { #chain.push_prop(#prop, (#value).to_string()) };
                }
            }
            StyleSyntax::Array => {
                let array: syn::ExprArray = syn::parse_str(expr)
                    .map_err(|e| format!("invalid `:style` array `{expr}`: {e}"))?;
                for elem in array.elems {
                    chain = quote! { #chain.push(#elem) };
                }
            }
            StyleSyntax::Plain => {
                let expr = parse_expr(expr)?;
                chain = quote! { #chain.push(#expr) };
            }
        }
    }
    Ok(Some(quote! { #chain.finish() }))
}

/// Parse a `:style` object literal `{ prop: value, .. }` into `(prop, value)`
/// pairs. A property name may be a bare camelCase token (`fontSize`, normalized
/// to `font-size`) or a quoted string (`'font-size'`, taken verbatim).
fn parse_style_object(expr: &str) -> Result<Vec<(String, String)>, String> {
    let inner = expr
        .trim()
        .strip_prefix('{')
        .and_then(|s| s.strip_suffix('}'))
        .ok_or_else(|| format!("invalid `:style` object `{expr}`"))?;
    let mut entries = Vec::new();
    for raw in split_top_level(inner, ',') {
        let entry = raw.trim();
        if entry.is_empty() {
            continue;
        }
        let colon = find_object_colon(entry)
            .ok_or_else(|| format!("`:style` object entry `{entry}` must be `prop: value`"))?;
        let prop = css_prop_name(entry[..colon].trim());
        let value = entry[colon + 1..].trim();
        if value.is_empty() {
            return Err(format!("`:style` object entry `{entry}` has no value"));
        }
        entries.push((prop, value.to_string()));
    }
    Ok(entries)
}

/// Normalize a `:style` object key to a CSS property name. A quoted key is taken
/// verbatim (after unquoting); a bare camelCase key is converted to kebab-case
/// (`fontSize` → `font-size`), matching Vue's `:style` key handling.
fn css_prop_name(key: &str) -> String {
    let key = key.trim();
    if matches!(key.chars().next(), Some('\'' | '"')) {
        return unquote(key);
    }
    let mut out = String::new();
    for c in key.chars() {
        if c.is_ascii_uppercase() {
            out.push('-');
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

/// Split `s` on top-level `delim`, ignoring delimiters nested inside brackets,
/// parentheses, braces, or string literals.
fn split_top_level(s: &str, delim: char) -> Vec<String> {
    let mut parts = Vec::new();
    let mut buf = String::new();
    let mut depth: i32 = 0;
    let mut string: Option<char> = None;
    let mut escaped = false;
    for c in s.chars() {
        if let Some(q) = string {
            buf.push(c);
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == q {
                string = None;
            }
            continue;
        }
        match c {
            '\'' | '"' => {
                string = Some(c);
                buf.push(c);
            }
            '(' | '[' | '{' => {
                depth += 1;
                buf.push(c);
            }
            ')' | ']' | '}' => {
                depth -= 1;
                buf.push(c);
            }
            _ if c == delim && depth == 0 => parts.push(std::mem::take(&mut buf)),
            _ => buf.push(c),
        }
    }
    parts.push(buf);
    parts
}

/// The byte index of the top-level `:` separating an object entry's name from its
/// condition. A `::` path separator is skipped so a path-bearing condition does
/// not capture the wrong colon.
fn find_object_colon(s: &str) -> Option<usize> {
    let mut depth: i32 = 0;
    let mut string: Option<char> = None;
    let mut escaped = false;
    let mut chars = s.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        if let Some(q) = string {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == q {
                string = None;
            }
            continue;
        }
        match c {
            '\'' | '"' => string = Some(c),
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            ':' if depth == 0 => {
                if matches!(chars.peek(), Some((_, ':'))) {
                    chars.next(); // a `::` path separator, not the entry's colon
                } else {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

/// Strip a single matching pair of surrounding quotes, if present.
fn unquote(s: &str) -> String {
    let mut chars = s.chars();
    if let (Some(first @ ('\'' | '"')), Some(last)) = (chars.next(), s.chars().last())
        && first == last
        && s.chars().count() >= 2
    {
        return s[first.len_utf8()..s.len() - last.len_utf8()].to_string();
    }
    s.to_string()
}

/// Whether `attr` sets the element's `class` (static `class="…"` or `:class="…"`).
fn is_class_attr(attr: &Attr) -> bool {
    match attr {
        Attr::Static { name, .. } | Attr::Dyn { name, .. } => name == "class",
        Attr::Event { .. } => false,
    }
}

/// Whether `attr` sets the element's `style` (static `style="…"` or `:style="…"`).
fn is_style_attr(attr: &Attr) -> bool {
    match attr {
        Attr::Static { name, .. } | Attr::Dyn { name, .. } => name == "style",
        Attr::Event { .. } => false,
    }
}

/// If `name` is a dynamic argument `[expr]modifiers` (produced by the parser for
/// `:[expr]` / `@[expr]`), return the inner `expr` and any trailing `.modifiers`.
/// A real attribute name never starts with `[`, so this is unambiguous.
fn dynamic_arg(name: &str) -> Option<(&str, &str)> {
    let rest = name.strip_prefix('[')?;
    let end = rest.find(']')?;
    Some((rest[..end].trim(), &rest[end + 1..]))
}

/// Split a bound attribute name into its base name and `.`-separated modifiers
/// (`value.prop` → `("value", ["prop"])`).
fn split_modifiers(name: &str) -> (&str, Vec<&str>) {
    let mut parts = name.split('.');
    let base = parts.next().unwrap_or("");
    (base, parts.collect())
}

/// Camelize a kebab-case bound name (`view-box` → `viewBox`), for the `.camel`
/// modifier.
fn camelize(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut upper_next = false;
    for c in name.chars() {
        if c == '-' {
            upper_next = true;
        } else if upper_next {
            out.extend(c.to_uppercase());
            upper_next = false;
        } else {
            out.push(c);
        }
    }
    out
}

fn is_structural(attr: &Attr) -> bool {
    match attr {
        Attr::Static { name, .. } => {
            matches!(name.as_str(), "v-if" | "v-else-if" | "v-else" | "v-for")
        }
        Attr::Dyn { name, .. } => name == "key",
        Attr::Event { .. } => false,
    }
}

fn find_static<'a>(el: &'a Element, name: &str) -> Option<&'a str> {
    el.attrs.iter().find_map(|attr| match attr {
        Attr::Static { name: n, value } if n == name => Some(value.as_str()),
        _ => None,
    })
}

fn find_dyn<'a>(el: &'a Element, name: &str) -> Option<&'a str> {
    el.attrs.iter().find_map(|attr| match attr {
        Attr::Dyn { name: n, expr } if n == name => Some(expr.as_str()),
        _ => None,
    })
}

fn parse_expr(src: &str) -> Result<syn::Expr, String> {
    syn::parse_str::<syn::Expr>(src).map_err(|e| format!("invalid expression `{src}`: {e}"))
}
