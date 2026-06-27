//! Lowers the template AST into `El`-builder Rust code.

use proc_macro2::TokenStream;
use quote::quote;

use crate::parser::{Attr, Element, Node};

/// Generate the builder expression for a template's single root element.
pub(crate) fn codegen_root(nodes: &[Node]) -> Result<TokenStream, String> {
    let mut elements = nodes.iter().filter_map(|n| match n {
        Node::Element(e) => Some(e),
        _ => None,
    });
    let root = elements.next();
    let has_extra = elements.next().is_some();
    let has_text = nodes.iter().any(|n| !matches!(n, Node::Element(_)));
    match root {
        Some(root) if !has_extra && !has_text => gen_element(root),
        _ => Err("template must have exactly one root element".to_string()),
    }
}

fn gen_element(el: &Element) -> Result<TokenStream, String> {
    let tag = &el.tag;
    let mut chain = quote! { El::new(__backend.clone(), #tag) };
    for attr in &el.attrs {
        if is_structural(attr) {
            continue; // v-if / v-else / v-for / :key are handled by the parent
        }
        let part = gen_attr(attr)?;
        chain = quote! { #chain #part };
    }
    for part in gen_children(&el.children)? {
        chain = quote! { #chain #part };
    }
    Ok(quote! { #chain.finish() })
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

fn gen_children(children: &[Node]) -> Result<Vec<TokenStream>, String> {
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
                    parts.push(gen_for(el, for_expr)?);
                } else if let Some(cond) = find_static(el, "v-if") {
                    if let Some(Node::Element(next)) = children.get(i + 1)
                        && find_static(next, "v-else").is_some()
                    {
                        parts.push(gen_if_else(el, next, cond)?);
                        i += 2;
                        continue;
                    }
                    parts.push(gen_if(el, cond)?);
                } else if find_static(el, "v-else").is_some() {
                    return Err("v-else without a matching v-if".to_string());
                } else {
                    let child = gen_element(el)?;
                    parts.push(quote! { .child(#child) });
                }
            }
        }
        i += 1;
    }
    Ok(parts)
}

fn gen_if(el: &Element, cond: &str) -> Result<TokenStream, String> {
    let cond = parse_expr(cond)?;
    let view = gen_element(el)?;
    Ok(quote! { .dyn_if(move || (#cond), move |__backend| #view) })
}

fn gen_if_else(then_el: &Element, else_el: &Element, cond: &str) -> Result<TokenStream, String> {
    let cond = parse_expr(cond)?;
    let then_view = gen_element(then_el)?;
    let else_view = gen_element(else_el)?;
    Ok(quote! {
        .dyn_if_else(move || (#cond), move |__backend| #then_view, move |__backend| #else_view)
    })
}

fn gen_for(el: &Element, for_expr: &str) -> Result<TokenStream, String> {
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
    let view = gen_element(el)?;
    Ok(quote! {
        .dyn_for(
            move || (#iterable),
            |#binding| (#key).clone(),
            move |__backend, #binding| #view,
        )
    })
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
