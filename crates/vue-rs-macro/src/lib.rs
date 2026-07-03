//! Procedural macros for vue-rs.

use std::collections::HashMap;

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

    // Partition the script. Lift `use` items and struct declarations to module
    // level so the `NameProps` type and any slot-payload structs can be named in
    // the render function's signature. The declared `NameSlots` struct (each
    // field is `name: Payload`) is metadata: it is replaced by a generated
    // `NameSlots<B>` whose fields are `Option<SlotFn<B, Payload>>`. Everything
    // else stays in the function body.
    let props_name = format!("{name}Props");
    let slots_name = format!("{name}Slots");
    let mut uses: Vec<syn::ItemUse> = Vec::new();
    let mut props_struct: Option<syn::ItemStruct> = None;
    let mut slots_struct: Option<syn::ItemStruct> = None;
    let mut structs: Vec<syn::ItemStruct> = Vec::new();
    let mut body: Vec<syn::Stmt> = Vec::new();
    if let Some(src) = sfc.script.as_deref().filter(|s| !s.is_empty()) {
        // Map Vue authoring spellings (e.g. the keyword `ref` → core `signal`)
        // before parsing, so the body can name the keyword-safe constructors.
        let desugared = match vue_rs_compiler::rewrite_script_sugar(src) {
            Ok(tokens) => tokens,
            Err(err) => return compile_error(&err.to_string()),
        };
        let block: syn::Block = match syn::parse2(quote! { { #desugared } }) {
            Ok(block) => block,
            Err(err) => return compile_error(&format!("invalid <script>: {err}")),
        };
        for stmt in block.stmts {
            match stmt {
                syn::Stmt::Item(syn::Item::Use(item)) => uses.push(item),
                syn::Stmt::Item(syn::Item::Struct(item)) if item.ident == props_name => {
                    props_struct = Some(item);
                }
                syn::Stmt::Item(syn::Item::Struct(item)) if item.ident == slots_name => {
                    slots_struct = Some(item);
                }
                syn::Stmt::Item(syn::Item::Struct(item)) => structs.push(item),
                other => body.push(other),
            }
        }
    }

    // Props are read-only: reject a declared props field whose type is a
    // writable handle (`Signal`/`WritableMemo`), so a child cannot mutate parent
    // state through a prop. Updates flow back up through emits.
    if let Some(Err(err)) = props_struct.as_ref().map(vue_rs_compiler::check_prop_fields) {
        return compile_error(&err.to_string());
    }

    // The declared `NameSlots` struct (if any) maps each scoped slot name to its
    // payload type; plain slots have no entry (their payload is `()`).
    let scoped_slots = slots_struct.as_ref().map(slot_fields).unwrap_or_default();
    let slots_ty = Ident::new(&slots_name, name.span());

    // A `<style>` block enables scoping: elements get a `data-v-<scope>` marker
    // and the CSS is rewritten to target it.
    let scope = sfc
        .style
        .as_ref()
        .map(|_| vue_rs_compiler::scope_id(&full_path_str));

    // Compile the template; it reports every `<slot>` it uses (and needs each
    // scoped slot's payload type to build the payload struct it hands the parent).
    let compiled = vue_rs_compiler::compile_component_template(
        &sfc.template,
        scope.as_deref(),
        scoped_slots.clone(),
    );
    let (template, used_slots) = match compiled {
        Ok(compiled) => (compiled.tokens, compiled.slots),
        Err(err) => return compile_error(&err.to_string()),
    };

    // Generate the component's `NameSlots` struct: one `Option<SlotFn<B, T>>`
    // field per slot the template uses (scoped slots use their declared payload
    // type, plain slots use `()`), plus a `with_<name>` builder per slot. Each
    // builder names its payload type, so the parent's slot closures need no
    // annotation; unprovided slots stay `None` and render their fallback. The
    // struct is always part of the signature, so the parent can call a
    // slot-bearing component without providing any slots at all.
    let scoped_map: HashMap<String, proc_macro2::TokenStream> = scoped_slots.into_iter().collect();
    let mut slot_defs: Vec<(Ident, proc_macro2::TokenStream)> = Vec::new();
    for (name, scoped) in &used_slots {
        let field = Ident::new(name, proc_macro2::Span::call_site());
        let payload = if *scoped {
            match scoped_map.get(name) {
                Some(payload) => payload.clone(),
                None => {
                    return compile_error(&format!(
                        "scoped slot `{name}` needs a `{name}: _` field in the component's {slots_name} struct"
                    ))
                }
            }
        } else {
            quote! { () }
        };
        slot_defs.push((field, payload));
    }
    let (slots_struct_def, slots_param) = gen_slots_struct(&slots_ty, &slot_defs);

    let props_param = props_struct.as_ref().map(|item| {
        let ty = &item.ident;
        let (_, ty_generics, _) = item.generics.split_for_impl();
        quote! { , props: #ty #ty_generics }
    });

    let style_const = sfc.style.as_ref().map(|css| {
        let scoped = vue_rs_compiler::scope_css(css, scope.as_deref().unwrap_or_default());
        let const_name = Ident::new(&format!("{}_STYLE", name).to_uppercase(), name.span());
        quote! { pub const #const_name: &str = #scoped; }
    });

    quote! {
        #(#uses)*
        #(#structs)*
        #props_struct
        #slots_struct_def
        #style_const

        #[allow(non_snake_case)]
        pub fn #name<B: ::vue_rs_dom::Backend>(
            __backend: B #props_param #slots_param
        ) -> B::Node {
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

/// Generate a component's `NameSlots` struct from its used slots (`(field,
/// payload)` pairs) and the function parameter that receives it. With no slots
/// it is a unit struct (still always passed, so every component call is uniform);
/// otherwise it is generic over the backend `B`, with an `Option<SlotFn<B, T>>`
/// field and a `with_<name>` builder per slot. `for_backend` pins `B` from a
/// value so the parent's slot closures can infer their parameter types.
fn gen_slots_struct(
    slots_ty: &Ident,
    slot_defs: &[(Ident, proc_macro2::TokenStream)],
) -> (proc_macro2::TokenStream, proc_macro2::TokenStream) {
    if slot_defs.is_empty() {
        let def = quote! {
            #[allow(non_camel_case_types)]
            pub struct #slots_ty;
            impl ::core::default::Default for #slots_ty {
                fn default() -> Self {
                    #slots_ty
                }
            }
            impl #slots_ty {
                pub fn for_backend<B: ::vue_rs_dom::Backend>(_backend: &B) -> Self {
                    #slots_ty
                }
            }
        };
        return (def, quote! { , __slots: #slots_ty });
    }
    let fields = slot_defs.iter().map(|(field, payload)| {
        quote! { pub #field: ::core::option::Option<::vue_rs_dom::SlotFn<B, #payload>> }
    });
    let defaults = slot_defs.iter().map(|(field, _)| {
        quote! { #field: ::core::option::Option::None }
    });
    let setters = slot_defs.iter().map(|(field, payload)| {
        let setter = Ident::new(&format!("with_{field}"), proc_macro2::Span::call_site());
        quote! {
            pub fn #setter(mut self, builder: impl Fn(B, #payload) -> B::Node + 'static) -> Self {
                self.#field = ::core::option::Option::Some(::vue_rs_dom::SlotFn::new(builder));
                self
            }
        }
    });
    let def = quote! {
        #[allow(non_camel_case_types)]
        pub struct #slots_ty<B: ::vue_rs_dom::Backend> {
            #(#fields),*
        }
        impl<B: ::vue_rs_dom::Backend> ::core::default::Default for #slots_ty<B> {
            fn default() -> Self {
                Self { #(#defaults),* }
            }
        }
        impl<B: ::vue_rs_dom::Backend> #slots_ty<B> {
            /// All slots unset; pins the backend type from a value so the
            /// parent's slot closures can infer their parameter types.
            pub fn for_backend(_backend: &B) -> Self {
                ::core::default::Default::default()
            }
            #(#setters)*
        }
    };
    (def, quote! { , __slots: #slots_ty<B> })
}

/// Each `name: Payload` field of a declared `NameSlots` struct gives a scoped
/// slot its payload type. Returns `(name, Payload)` pairs.
fn slot_fields(slots: &syn::ItemStruct) -> Vec<(String, proc_macro2::TokenStream)> {
    let syn::Fields::Named(fields) = &slots.fields else {
        return Vec::new();
    };
    let mut out: Vec<(String, proc_macro2::TokenStream)> = fields
        .named
        .iter()
        .filter_map(|field| {
            let name = field.ident.as_ref()?;
            let payload = &field.ty;
            Some((name.to_string(), quote! { #payload }))
        })
        .collect();
    out.sort_by(|(a, _), (b, _)| a.cmp(b));
    out
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
