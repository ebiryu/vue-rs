//! Procedural macros for vue-rs.

use proc_macro::TokenStream;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::{parse_macro_input, Expr, Ident, LitStr, Token};

/// `view!(backend, "<template>")` compiles a Vue-style template literal into
/// `El`-builder code, binding the given backend expression as the render target.
///
/// The template's `{{ }}`, `:attr`, and `@event` expressions resolve against the
/// bindings in scope at the call site.
#[proc_macro]
pub fn view(input: TokenStream) -> TokenStream {
    let ViewInput { backend, template } = parse_macro_input!(input as ViewInput);
    match vue_rs_compiler::compile_template(&template.value()) {
        Ok(body) => quote! {{
            let __backend = #backend;
            #body
        }}
        .into(),
        Err(err) => {
            let message = err.to_string();
            quote! { compile_error!(#message) }.into()
        }
    }
}

/// `component!(name, "path/to/file.vrs")` reads a single-file component, splices
/// its `<script lang="rust">` and the code compiled from its `<template>` into a
/// single generic render function `fn name<B: Backend>(__backend: B) -> B::Node`.
///
/// If a `<style>` block is present, its CSS is exposed as `pub const NAME_STYLE`.
#[proc_macro]
pub fn component(input: TokenStream) -> TokenStream {
    let ComponentInput { name, path } = parse_macro_input!(input as ComponentInput);

    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set");
    let full_path = std::path::Path::new(&manifest_dir).join(path.value());
    let full_path_str = full_path.to_string_lossy().into_owned();

    let source = match std::fs::read_to_string(&full_path) {
        Ok(source) => source,
        Err(err) => return compile_error(&format!("cannot read {full_path_str}: {err}")),
    };

    let sfc = match vue_rs_compiler::split_sfc(&source) {
        Ok(sfc) => sfc,
        Err(err) => return compile_error(&err.to_string()),
    };

    let script: proc_macro2::TokenStream = match sfc.script.as_deref().unwrap_or("").parse() {
        Ok(tokens) => tokens,
        Err(err) => return compile_error(&format!("invalid <script>: {err}")),
    };

    let template = match vue_rs_compiler::compile_template(&sfc.template) {
        Ok(tokens) => tokens,
        Err(err) => return compile_error(&err.to_string()),
    };

    let style_const = sfc.style.map(|css| {
        let const_name = Ident::new(&format!("{}_STYLE", name).to_uppercase(), name.span());
        quote! { pub const #const_name: &str = #css; }
    });

    quote! {
        #style_const

        pub fn #name<B: ::vue_rs_dom::Backend>(__backend: B) -> B::Node {
            // Re-run this macro when the source file changes.
            const _: &[u8] = include_bytes!(#full_path_str);
            use ::vue_rs_dom::El;
            #script
            #template
        }
    }
    .into()
}

fn compile_error(message: &str) -> TokenStream {
    quote! { compile_error!(#message); }.into()
}

struct ComponentInput {
    name: Ident,
    path: LitStr,
}

impl Parse for ComponentInput {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let name = input.parse()?;
        input.parse::<Token![,]>()?;
        let path = input.parse()?;
        Ok(ComponentInput { name, path })
    }
}

struct ViewInput {
    backend: Expr,
    template: LitStr,
}

impl Parse for ViewInput {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let backend = input.parse()?;
        input.parse::<Token![,]>()?;
        let template = input.parse()?;
        Ok(ViewInput { backend, template })
    }
}
