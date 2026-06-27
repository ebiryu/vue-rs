//! Lowers the template AST into `El`-builder Rust code.

use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::Ident;

use crate::parser::{Attr, Element, Node};

/// Code generator. When `scope` is set (a `<style scoped>` is present), every
/// element gets a `data-v-<scope>` marker attribute so scoped CSS can target it.
pub(crate) struct Codegen {
    scope: Option<String>,
}

impl Codegen {
    pub(crate) fn new(scope: Option<String>) -> Self {
        Self { scope }
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
            // Render the named slot's content, or an empty anchor if absent.
            let name = find_static(el, "name").unwrap_or("default");
            return Ok(quote! {
                __slots
                    .render(#name, __backend.clone())
                    .unwrap_or_else(|| ::vue_rs_dom::Backend::create_anchor(&__backend))
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
        for attr in &el.attrs {
            if is_structural(attr) {
                continue; // v-if / v-else / v-for / :key are handled by the parent
            }
            let part = gen_attr(attr)?;
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
        let mut args = vec![quote! { __backend.clone() }];
        if !fields.is_empty() {
            let props = Ident::new(&format!("{}Props", el.tag), Span::call_site());
            args.push(quote! { #props { #(#fields),* } });
        }
        if let Some(slots) = self.slots(&el.children)? {
            args.push(slots);
        }
        Ok(quote! { #component(#(#args),*) })
    }

    /// Build the `Slots` a parent passes to a component: `<template v-slot:name>`
    /// children become named slots; the remaining single element is the default.
    fn slots(&self, children: &[Node]) -> Result<Option<TokenStream>, String> {
        let elements: Vec<&Element> = children
            .iter()
            .filter_map(|n| match n {
                Node::Element(e) => Some(e),
                _ => None,
            })
            .collect();
        if elements.is_empty() {
            return Ok(None);
        }
        let mut builder = quote! { ::vue_rs_dom::Slots::for_backend(&__backend) };
        let mut default_content: Vec<&Element> = Vec::new();
        for el in elements {
            if el.tag == "template"
                && let Some(name) = slot_name(el)
            {
                let content = self.single_root(&el.children)?;
                builder = quote! { #builder.with(#name, move |__backend| #content) };
            } else {
                default_content.push(el);
            }
        }
        match default_content.as_slice() {
            [] => {}
            [only] => {
                let content = self.element(only)?;
                builder = quote! { #builder.with("default", move |__backend| #content) };
            }
            _ => return Err("default slot content must be a single root element".to_string()),
        }
        Ok(Some(builder))
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
        let binding: TokenStream = binding
            .trim()
            .parse()
            .map_err(|e| format!("invalid v-for binding: {e}"))?;
        let iterable = parse_expr(iterable.trim())?;
        let key = find_dyn(el, "key").ok_or("v-for requires a :key binding")?;
        let key = parse_expr(key)?;
        let view = self.element(el)?;
        Ok(quote! {
            .dyn_for(
                move || (#iterable),
                |#binding| (#key).clone(),
                move |__backend, #binding| #view,
            )
        })
    }
}

fn is_component(tag: &str) -> bool {
    tag.chars().next().is_some_and(|c| c.is_ascii_uppercase())
}

/// The slot name from a `<template v-slot:name>` element, if present.
fn slot_name(el: &Element) -> Option<&str> {
    el.attrs.iter().find_map(|attr| match attr {
        Attr::Static { name, .. } => name.strip_prefix("v-slot:"),
        _ => None,
    })
}

fn gen_attr(attr: &Attr) -> Result<TokenStream, String> {
    match attr {
        Attr::Static { name, value } if name == "v-model" => {
            let model = parse_expr(value)?;
            Ok(quote! {
                .dyn_attr("value", move || ((#model).get()).to_string())
                .on_value("input", move |__value| (#model).set(__value.to_string()))
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
