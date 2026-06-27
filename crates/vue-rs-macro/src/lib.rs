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

/// `component!(name, "path/to/file.vrs")` reads a single-file component and
/// produces a render function `fn name<B: Backend>(__backend: B[, props: NameProps]) -> B::Node`,
/// splicing the `<script lang="rust">` body and the code compiled from `<template>`.
///
/// If the `<script>` declares a `struct NameProps { .. }`, the function gains a
/// `props: NameProps` parameter (props/emits flow in through it). `use` items and
/// that struct are lifted to module level so the signature can name the type.
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

    // Partition the script: lift `use` items and the `NameProps` struct to module
    // level; keep the rest as the function body.
    let props_name = format!("{name}Props");
    let mut uses: Vec<syn::ItemUse> = Vec::new();
    let mut props_struct: Option<syn::ItemStruct> = None;
    let mut body: Vec<syn::Stmt> = Vec::new();
    if let Some(src) = sfc.script.as_deref().filter(|s| !s.is_empty()) {
        let block: syn::Block = match syn::parse_str(&format!("{{ {src} }}")) {
            Ok(block) => block,
            Err(err) => return compile_error(&format!("invalid <script>: {err}")),
        };
        for stmt in block.stmts {
            match stmt {
                syn::Stmt::Item(syn::Item::Use(item)) => uses.push(item),
                syn::Stmt::Item(syn::Item::Struct(item)) if item.ident == props_name => {
                    props_struct = Some(item);
                }
                other => body.push(other),
            }
        }
    }

    // A `<style>` block enables scoping: elements get a `data-v-<scope>` marker
    // and the CSS is rewritten to target it.
    let scope = sfc
        .style
        .as_ref()
        .map(|_| vue_rs_compiler::scope_id(&full_path_str));

    let compiled = match &scope {
        Some(scope) => vue_rs_compiler::compile_template_scoped(&sfc.template, scope),
        None => vue_rs_compiler::compile_template(&sfc.template),
    };
    let template = match compiled {
        Ok(tokens) => tokens,
        Err(err) => return compile_error(&err.to_string()),
    };

    let props_param = props_struct.as_ref().map(|item| {
        let ty = &item.ident;
        quote! { , props: #ty }
    });

    // A `<slot>` in the template means the component accepts parent-provided
    // slot content, passed as a `Slots` map.
    let slots_param = sfc
        .template
        .contains("<slot")
        .then(|| quote! { , __slots: ::vue_rs_dom::Slots<B> });

    let style_const = sfc.style.as_ref().map(|css| {
        let scoped = vue_rs_compiler::scope_css(css, scope.as_deref().unwrap_or_default());
        let const_name = Ident::new(&format!("{}_STYLE", name).to_uppercase(), name.span());
        quote! { pub const #const_name: &str = #scoped; }
    });

    quote! {
        #(#uses)*
        #props_struct
        #style_const

        #[allow(non_snake_case)]
        pub fn #name<B: ::vue_rs_dom::Backend>(__backend: B #props_param #slots_param) -> B::Node {
            // Re-run this macro when the source file changes.
            const _: &[u8] = include_bytes!(#full_path_str);
            use ::vue_rs_dom::El;
            // Each component owns a scope: its effects and provided contexts are
            // scoped to this subtree.
            ::vue_rs_reactive::run_in_child_scope(move || {
                #(#body)*
                #template
            })
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
