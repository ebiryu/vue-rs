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
        let part = gen_attr(attr)?;
        chain = quote! { #chain #part };
    }
    for child in &el.children {
        let part = gen_child(child)?;
        chain = quote! { #chain #part };
    }
    Ok(quote! { #chain.finish() })
}

fn gen_attr(attr: &Attr) -> Result<TokenStream, String> {
    match attr {
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

fn gen_child(node: &Node) -> Result<TokenStream, String> {
    match node {
        Node::StaticText(text) => Ok(quote! { .text(#text) }),
        Node::DynText(expr) => {
            let expr = parse_expr(expr)?;
            Ok(quote! { .dyn_text(move || (#expr).to_string()) })
        }
        Node::Element(el) => {
            let child = gen_element(el)?;
            Ok(quote! { .child(#child) })
        }
    }
}

fn parse_expr(src: &str) -> Result<syn::Expr, String> {
    syn::parse_str::<syn::Expr>(src).map_err(|e| format!("invalid expression `{src}`: {e}"))
}
