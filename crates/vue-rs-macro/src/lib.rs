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

    // Emit the (cleaned) `NameProps` struct plus its typestate builder. The
    // parent passes props by name through the builder; required props stay
    // checked at compile time, and `#[prop(default)]` fields become optional.
    let props_defs = match props_struct.as_ref() {
        Some(item) => match gen_props_builder(item) {
            Ok(defs) => defs,
            Err(err) => return compile_error(&err),
        },
        None => quote! {},
    };

    let props_param = props_struct.as_ref().map(|item| {
        let ty = &item.ident;
        quote! { , props: #ty }
    });

    let style_const = sfc.style.as_ref().map(|css| {
        let scoped = vue_rs_compiler::scope_css(css, scope.as_deref().unwrap_or_default());
        let const_name = Ident::new(&format!("{}_STYLE", name).to_uppercase(), name.span());
        quote! { pub const #const_name: &str = #scoped; }
    });

    quote! {
        #(#uses)*
        #(#structs)*
        #props_defs
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
/// the same nodes but exposes no writes.
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
    })
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

/// How an optional prop's value is filled when the parent omits it.
enum PropDefault {
    /// `#[prop(default)]`: use `Default::default()`.
    Auto,
    /// `#[prop(default = expr)]`: use `expr`.
    Expr(Expr),
}

/// Parse a field's `#[prop(default[= expr])]` attribute, if present. A field
/// with such an attribute is optional; without one it is required.
fn parse_prop_default(field: &syn::Field) -> Result<Option<PropDefault>, String> {
    let mut found = None;
    for attr in &field.attrs {
        if !attr.path().is_ident("prop") {
            continue;
        }
        let parsed = attr
            .parse_args_with(|input: ParseStream| {
                let kw: Ident = input.parse()?;
                if kw != "default" {
                    return Err(syn::Error::new(kw.span(), "expected `default`"));
                }
                if input.peek(Token![=]) {
                    input.parse::<Token![=]>()?;
                    Ok(PropDefault::Expr(input.parse()?))
                } else {
                    Ok(PropDefault::Auto)
                }
            })
            .map_err(|e| format!("invalid `#[prop(...)]` on `{}`: {e}", field.ident.as_ref().unwrap()))?;
        found = Some(parsed);
    }
    Ok(found)
}

/// Generate a component's props type: the `NameProps` struct (with `#[prop]`
/// attributes stripped) plus a typestate builder the parent uses to pass props
/// by name.
///
/// Each *required* prop carries a marker type parameter that its setter flips
/// from `Unset` to `Set`; `build()` exists only when every marker is `Set`, so
/// omitting a required prop leaves the `.build()` call uncompilable — required
/// props stay checked at compile time. A prop marked `#[prop(default)]` or
/// `#[prop(default = expr)]` is optional: it has no marker, and `build()` fills
/// the default when the parent omits it.
fn gen_props_builder(item: &syn::ItemStruct) -> Result<proc_macro2::TokenStream, String> {
    if !item.generics.params.is_empty() {
        return Err(format!(
            "generic props struct `{}` is not supported",
            item.ident
        ));
    }
    let syn::Fields::Named(named) = &item.fields else {
        return Err(format!("props struct `{}` must have named fields", item.ident));
    };

    let ty = &item.ident;
    let builder_ty = Ident::new(&format!("{ty}Builder"), ty.span());

    // Classify each field as required or optional (with its default).
    struct FieldInfo {
        ident: Ident,
        ty: syn::Type,
        default: Option<PropDefault>,
    }
    let mut fields: Vec<FieldInfo> = Vec::new();
    for field in &named.named {
        fields.push(FieldInfo {
            ident: field.ident.clone().unwrap(),
            ty: field.ty.clone(),
            default: parse_prop_default(field)?,
        });
    }

    // One marker type parameter per required field, in declaration order.
    let marker_params: Vec<Ident> = fields
        .iter()
        .filter(|f| f.default.is_none())
        .enumerate()
        .map(|(i, _)| Ident::new(&format!("__M{i}"), proc_macro2::Span::call_site()))
        .collect();
    let k = marker_params.len();
    let unset = quote! { ::vue_rs_dom::builder::Unset };
    let set = quote! { ::vue_rs_dom::builder::Set };

    // Angle-bracket a marker list, or nothing when there are no required fields.
    let generics = |markers: Vec<proc_macro2::TokenStream>| {
        if markers.is_empty() {
            quote! {}
        } else {
            quote! { <#(#markers),*> }
        }
    };
    let builder_all_unset = {
        let g = generics((0..k).map(|_| unset.clone()).collect());
        quote! { #builder_ty #g }
    };
    let builder_all_set = {
        let g = generics((0..k).map(|_| set.clone()).collect());
        quote! { #builder_ty #g }
    };

    // The builder holds every field as `Option`, plus a `PhantomData` over the
    // markers so required-field state is tracked in the type.
    let builder_fields = fields.iter().map(|f| {
        let (ident, fty) = (&f.ident, &f.ty);
        quote! { #ident: ::core::option::Option<#fty> }
    });
    let markers_field = (k > 0).then(|| {
        quote! { , __markers: ::core::marker::PhantomData<(#(#marker_params),*)> }
    });
    let markers_init = (k > 0).then(|| quote! { , __markers: ::core::marker::PhantomData });
    let struct_generics = generics(marker_params.iter().map(|m| quote! { #m }).collect());

    let none_inits = fields.iter().map(|f| {
        let ident = &f.ident;
        quote! { #ident: ::core::option::Option::None }
    });

    // A setter per field. Required setters flip their marker `Unset` → `Set`
    // (generic over the other markers); optional setters leave markers alone.
    let mut req_seen = 0usize;
    let setters = fields.iter().map(|f| {
        let (ident, fty) = (&f.ident, &f.ty);
        if f.default.is_some() {
            // Optional: no marker change.
            let g = generics(marker_params.iter().map(|m| quote! { #m }).collect());
            let self_ty = generics(marker_params.iter().map(|m| quote! { #m }).collect());
            quote! {
                impl #g #builder_ty #self_ty {
                    #[allow(dead_code)]
                    pub fn #ident(mut self, value: #fty) -> Self {
                        self.#ident = ::core::option::Option::Some(value);
                        self
                    }
                }
            }
        } else {
            // Required: marker at this position goes `Unset` → `Set`.
            let pos = req_seen;
            req_seen += 1;
            let others: Vec<&Ident> = marker_params
                .iter()
                .enumerate()
                .filter(|(j, _)| *j != pos)
                .map(|(_, m)| m)
                .collect();
            let in_markers: Vec<proc_macro2::TokenStream> = marker_params
                .iter()
                .enumerate()
                .map(|(j, m)| if j == pos { unset.clone() } else { quote! { #m } })
                .collect();
            let out_markers: Vec<proc_macro2::TokenStream> = marker_params
                .iter()
                .enumerate()
                .map(|(j, m)| if j == pos { set.clone() } else { quote! { #m } })
                .collect();
            let rest = fields.iter().filter(|g| &g.ident != ident).map(|g| {
                let id = &g.ident;
                quote! { #id: self.#id }
            });
            quote! {
                impl<#(#others),*> #builder_ty<#(#in_markers),*> {
                    #[allow(dead_code)]
                    pub fn #ident(self, value: #fty) -> #builder_ty<#(#out_markers),*> {
                        #builder_ty {
                            #ident: ::core::option::Option::Some(value),
                            #(#rest,)*
                            __markers: ::core::marker::PhantomData
                        }
                    }
                }
            }
        }
    });

    // `build()` is reachable only when every required marker is `Set`.
    let build_inits = fields.iter().map(|f| {
        let ident = &f.ident;
        match &f.default {
            None => quote! { #ident: ::core::option::Option::unwrap(self.#ident) },
            Some(PropDefault::Auto) => quote! {
                #ident: ::core::option::Option::unwrap_or_else(
                    self.#ident, || ::core::default::Default::default())
            },
            Some(PropDefault::Expr(expr)) => quote! {
                #ident: ::core::option::Option::unwrap_or_else(self.#ident, || #expr)
            },
        }
    });

    // The struct itself, with `#[prop]` attributes removed so it is valid Rust.
    let mut cleaned = item.clone();
    if let syn::Fields::Named(named) = &mut cleaned.fields {
        for field in &mut named.named {
            field.attrs.retain(|a| !a.path().is_ident("prop"));
        }
    }

    Ok(quote! {
        #cleaned

        #[allow(non_camel_case_types)]
        pub struct #builder_ty #struct_generics {
            #(#builder_fields),*
            #markers_field
        }

        impl #ty {
            #[allow(dead_code)]
            pub fn builder() -> #builder_all_unset {
                #builder_ty {
                    #(#none_inits),*
                    #markers_init
                }
            }
        }

        #(#setters)*

        impl #builder_all_set {
            #[allow(dead_code)]
            pub fn build(self) -> #ty {
                #ty { #(#build_inits),* }
            }
        }
    })
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
