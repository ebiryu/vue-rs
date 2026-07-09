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

    // `name` here is the parsed `Ident`, so its span is preserved. The file path
    // seeds the scope id and drives the `include_bytes!` rebuild trigger.
    match vue_rs_compiler::compile_component(&name, &source, &full_path_str, Some(&full_path_str)) {
        Ok(tokens) => tokens.into(),
        Err(err) => compile_error(&err.to_string()),
    }
}

fn compile_error(message: &str) -> TokenStream {
    quote! { compile_error!(#message); }.into()
}

/// `#[derive(Reactive)]` turns a plain struct into a fine-grained reactive
/// companion: for `struct State { count: i32 }` it generates
/// `struct StateReactive { count: Signal<i32> }` and an
/// `impl Reactive for State` so `reactive(State { .. })` yields the companion.
///
/// The companion is `Copy` (every field is a `Signal` handle) and mirrors each
/// field's visibility. Every field type must be `PartialEq + 'static` (enforced
/// where each `signal(..)` is generated). Only structs with named fields are
/// supported; tuple/unit structs, enums, unions, and generics are rejected.
///
/// A field annotated `#[reactive]` is itself a `#[derive(Reactive)]` type: its
/// companion is embedded recursively (as `<Field as Reactive>::Target`, built
/// with `reactive(..)`) instead of `Signal<Field>`, so nested fields stay
/// independently tracked.
///
/// It also generates a read-only view `StateReadonly` (each writable handle
/// projected to a read-only one, recursively) and `impl Readonly for
/// StateReactive`, so `readonly(reactive(State { .. }))` yields a view that reads
/// the same nodes but exposes no writes. A `From<StateReactive> for StateReadonly`
/// lets the companion flow down as its read-only view where one is expected — a
/// child prop declared `StateReadonly` receives the companion converted through
/// `Into::into`, so a composite reactive value stays read-only across the prop
/// boundary (one-way data flow).
#[proc_macro_derive(Reactive, attributes(reactive))]
pub fn derive_reactive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as syn::DeriveInput);
    match gen_reactive(&input) {
        Ok(tokens) => tokens.into(),
        Err(message) => compile_error(&message),
    }
}

/// A field marked `#[reactive]` is itself a `#[derive(Reactive)]` type: its
/// companion is embedded recursively (via [`Reactive::Target`]) instead of being
/// wrapped in a single `Signal<Field>`, so nested fields stay independently
/// tracked.
fn is_reactive_field(f: &syn::Field) -> bool {
    f.attrs.iter().any(|a| a.path().is_ident("reactive"))
}

fn gen_reactive(input: &syn::DeriveInput) -> Result<proc_macro2::TokenStream, String> {
    if !input.generics.params.is_empty() {
        return Err("#[derive(Reactive)] does not support generic structs".into());
    }
    let fields = match &input.data {
        syn::Data::Struct(data) => match &data.fields {
            syn::Fields::Named(named) => &named.named,
            _ => {
                return Err(
                    "#[derive(Reactive)] requires a struct with named fields".into(),
                )
            }
        },
        _ => return Err("#[derive(Reactive)] can only be applied to structs".into()),
    };

    let name = &input.ident;
    let vis = &input.vis;
    let companion = quote::format_ident!("{}Reactive", name);
    let readonly = quote::format_ident!("{}Readonly", name);

    // The type of each companion field: `Signal<T>` for a plain field, or the
    // nested companion `<T as Reactive>::Target` for a `#[reactive]` field.
    let companion_field_ty = |f: &syn::Field| -> proc_macro2::TokenStream {
        let fty = &f.ty;
        if is_reactive_field(f) {
            quote! { <#fty as ::vue_rs_reactive::Reactive>::Target }
        } else {
            quote! { ::vue_rs_reactive::Signal<#fty> }
        }
    };

    let companion_fields = fields.iter().map(|f| {
        let fvis = &f.vis;
        let fname = f.ident.as_ref().expect("named field");
        let fcty = companion_field_ty(f);
        quote! { #fvis #fname: #fcty }
    });

    let inits = fields.iter().map(|f| {
        let fname = f.ident.as_ref().expect("named field");
        if is_reactive_field(f) {
            quote! { #fname: ::vue_rs_reactive::reactive(self.#fname) }
        } else {
            quote! { #fname: ::vue_rs_reactive::signal(self.#fname) }
        }
    });

    // The read-only view mirrors the companion with every writable handle
    // projected read-only (`<CompanionFieldTy as Readonly>::Target`), built by
    // `readonly(..)` on each field. `Signal<T>` -> `ReadSignal<T>`, nested
    // companions -> their own read-only view.
    let readonly_fields = fields.iter().map(|f| {
        let fvis = &f.vis;
        let fname = f.ident.as_ref().expect("named field");
        let fcty = companion_field_ty(f);
        quote! { #fvis #fname: <#fcty as ::vue_rs_reactive::Readonly>::Target }
    });

    let readonly_inits = fields.iter().map(|f| {
        let fname = f.ident.as_ref().expect("named field");
        quote! { #fname: ::vue_rs_reactive::readonly(self.#fname) }
    });

    Ok(quote! {
        #[derive(Clone, Copy)]
        #vis struct #companion {
            #(#companion_fields,)*
        }

        #[derive(Clone, Copy)]
        #vis struct #readonly {
            #(#readonly_fields,)*
        }

        impl ::vue_rs_reactive::Reactive for #name {
            type Target = #companion;
            fn into_reactive(self) -> Self::Target {
                #companion {
                    #(#inits,)*
                }
            }
        }

        impl ::vue_rs_reactive::Readonly for #companion {
            type Target = #readonly;
            fn into_readonly(self) -> Self::Target {
                #readonly {
                    #(#readonly_inits,)*
                }
            }
        }

        // Let a mutable companion flow down as its read-only view where one is
        // expected (e.g. a child's prop), so `Into::into` in codegen picks it
        // up — mirroring `From<Signal<T>> for ReadSignal<T>`.
        impl ::core::convert::From<#companion> for #readonly {
            fn from(companion: #companion) -> Self {
                ::vue_rs_reactive::readonly(companion)
            }
        }
    })
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
