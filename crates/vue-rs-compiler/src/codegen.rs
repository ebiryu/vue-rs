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

    /// Generate the builder expression for a template's single root element.
    pub(crate) fn root(&self, nodes: &[Node]) -> Result<TokenStream, String> {
        let mut elements = nodes.iter().filter_map(|n| match n {
            Node::Element(e) => Some(e),
            _ => None,
        });
        let root = elements.next();
        let has_extra = elements.next().is_some();
        let has_text = nodes.iter().any(|n| !matches!(n, Node::Element(_)));
        match root {
            Some(root) if !has_extra && !has_text => self.element(root),
            _ => Err("template must have exactly one root element".to_string()),
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
            Some(if let Some(value) = find_static(el, "style") {
                quote! { #value.to_string() }
            } else if let Some(expr) = find_dyn(el, "style") {
                let expr = parse_expr(expr)?;
                quote! { (#expr).to_string() }
            } else {
                quote! { ::std::string::String::new() }
            })
        } else {
            None
        };
        for attr in &el.attrs {
            if is_structural(attr) {
                continue; // v-if / v-else / v-for / :key are handled by the parent
            }
            if base_style.is_some() && is_style_attr(attr) {
                continue; // folded into the `v-show` toggle below
            }
            let part = gen_attr(attr, base_style.as_ref())?;
            chain = quote! { #chain #part };
        }
        for part in self.children(&el.children)? {
            chain = quote! { #chain #part };
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
                Attr::Dyn { name, expr } => {
                    let field = Ident::new(name, Span::call_site());
                    let expr = parse_expr(expr)?;
                    fields.push(quote! { #field: #expr });
                }
                Attr::Event { name, handler } => {
                    let field = Ident::new(&format!("on_{name}"), Span::call_site());
                    let handler = parse_expr(handler)?;
                    fields.push(quote! { #field: ::vue_rs_dom::Callback::new(#handler) });
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
                        if let Some(Node::Element(next)) = children.get(i + 1)
                            && find_static(next, "v-else").is_some()
                        {
                            parts.push(self.gen_if_else(el, next, cond)?);
                            i += 2;
                            continue;
                        }
                        parts.push(self.gen_if(el, cond)?);
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

    fn gen_if(&self, el: &Element, cond: &str) -> Result<TokenStream, String> {
        let cond = parse_expr(cond)?;
        let view = self.element(el)?;
        Ok(quote! { .dyn_if(move || (#cond), move |__backend| #view) })
    }

    fn gen_if_else(
        &self,
        then_el: &Element,
        else_el: &Element,
        cond: &str,
    ) -> Result<TokenStream, String> {
        let cond = parse_expr(cond)?;
        let then_view = self.element(then_el)?;
        let else_view = self.element(else_el)?;
        Ok(quote! {
            .dyn_if_else(move || (#cond), move |__backend| #then_view, move |__backend| #else_view)
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

/// The slot name and binding pattern from a `<template v-slot:name="pat">`
/// element, if present. The binding is empty for a plain `<template v-slot:name>`.
fn slot_directive(el: &Element) -> Option<(&str, &str)> {
    el.attrs.iter().find_map(|attr| match attr {
        Attr::Static { name, value } => name.strip_prefix("v-slot:").map(|n| (n, value.as_str())),
        _ => None,
    })
}

fn gen_attr(attr: &Attr, base_style: Option<&TokenStream>) -> Result<TokenStream, String> {
    match attr {
        Attr::Static { name, value } if name == "v-model" => {
            let model = parse_expr(value)?;
            Ok(quote! {
                .dyn_attr("value", move || ((#model).get()).to_string())
                .on_value("input", move |__value| (#model).set(__value.to_string()))
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
        Attr::Static { name, value } => Ok(quote! { .attr(#name, #value) }),
        Attr::Dyn { name, expr } => {
            let expr = parse_expr(expr)?;
            Ok(quote! { .dyn_attr(#name, move || (#expr).to_string()) })
        }
        Attr::Event { name, handler } => {
            let handler = parse_expr(handler)?;
            Ok(quote! { .on(#name, move || { #handler }) })
        }
    }
}

/// Whether `attr` sets the element's `style` (static `style="…"` or `:style="…"`).
fn is_style_attr(attr: &Attr) -> bool {
    match attr {
        Attr::Static { name, .. } | Attr::Dyn { name, .. } => name == "style",
        Attr::Event { .. } => false,
    }
}

fn is_structural(attr: &Attr) -> bool {
    match attr {
        Attr::Static { name, .. } => matches!(name.as_str(), "v-if" | "v-else" | "v-for"),
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
