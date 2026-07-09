//! Whole-component codegen: turns a parsed `.vrs` source into the
//! `pub fn Name<B: Backend>` render function plus its props builder, slots
//! struct, hoisted items, and scoped-style const.

use std::collections::HashMap;

use proc_macro2::{Ident, Span, TokenStream};
use quote::quote;
use syn::parse::ParseStream;
use syn::{Expr, Token};

use crate::CompileError;

/// Compile a whole `.vrs` source into a component: the `pub fn Name<B: Backend>`
/// render function plus its `NameProps` builder, `NameSlots` struct, hoisted
/// `use`/`struct` items, and scoped-style const. `scope_seed` is hashed for the
/// `data-v-*` scope id (pass a stable per-file path). When `rerun_path` is
/// `Some`, a `const _: &[u8] = include_bytes!(path);` rebuild trigger is emitted
/// inside the render fn (proc-macro use); pass `None` from a build script that
/// tracks changes another way.
pub fn compile_component(
    name: &Ident,
    source: &str,
    scope_seed: &str,
    rerun_path: Option<&str>,
) -> Result<TokenStream, CompileError> {
    let sfc = crate::split_sfc(source)?;

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
        let desugared = crate::rewrite_script_sugar(src)?;
        let block: syn::Block = match syn::parse2(quote! { { #desugared } }) {
            Ok(block) => block,
            Err(err) => return Err(CompileError(format!("invalid <script>: {err}"))),
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
    if let Some(props) = props_struct.as_ref() {
        crate::check_prop_fields(props)?;
    }

    // The declared `NameSlots` struct (if any) maps each scoped slot name to its
    // payload type; plain slots have no entry (their payload is `()`).
    let scoped_slots = slots_struct.as_ref().map(slot_fields).unwrap_or_default();
    let slots_ty = Ident::new(&slots_name, name.span());

    // A `<style>` block enables scoping: elements get a `data-v-<scope>` marker
    // and the CSS is rewritten to target it.
    let scope = sfc.style.as_ref().map(|_| crate::scope_id(scope_seed));

    // Compile the template; it reports every `<slot>` it uses (and needs each
    // scoped slot's payload type to build the payload struct it hands the parent).
    let compiled = crate::compile_component_template(
        &sfc.template,
        scope.as_deref(),
        scoped_slots.clone(),
    )?;
    let (template, used_slots) = (compiled.tokens, compiled.slots);

    // Generate the component's `NameSlots` struct: one `Option<SlotFn<B, T>>`
    // field per slot the template uses (scoped slots use their declared payload
    // type, plain slots use `()`), plus a `with_<name>` builder per slot. Each
    // builder names its payload type, so the parent's slot closures need no
    // annotation; unprovided slots stay `None` and render their fallback. The
    // struct is always part of the signature, so the parent can call a
    // slot-bearing component without providing any slots at all.
    let scoped_map: HashMap<String, TokenStream> = scoped_slots.into_iter().collect();
    let mut slot_defs: Vec<(Ident, TokenStream)> = Vec::new();
    for (slot, scoped) in &used_slots {
        let field = Ident::new(slot, Span::call_site());
        let payload = if *scoped {
            match scoped_map.get(slot) {
                Some(payload) => payload.clone(),
                None => {
                    return Err(CompileError(format!(
                        "scoped slot `{slot}` needs a `{slot}: _` field in the component's {slots_name} struct"
                    )))
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
        Some(item) => gen_props_builder(item).map_err(CompileError)?,
        None => quote! {},
    };

    let props_param = props_struct.as_ref().map(|item| {
        let ty = &item.ident;
        quote! { , props: #ty }
    });

    let style_const = sfc.style.as_ref().map(|css| {
        let scoped = crate::scope_css(css, scope.as_deref().unwrap_or_default());
        let const_name = Ident::new(&format!("{}_STYLE", name).to_uppercase(), name.span());
        quote! { pub const #const_name: &str = #scoped; }
    });

    // Re-run this codegen when the source file changes (proc-macro path only).
    let rerun = rerun_path.map(|p| quote! { const _: &[u8] = include_bytes!(#p); });

    Ok(quote! {
        #(#uses)*
        #(#structs)*
        #props_defs
        #slots_struct_def
        #style_const

        #[allow(non_snake_case)]
        pub fn #name<B: ::vue_rs_dom::Backend>(
            __backend: B #props_param #slots_param
        ) -> B::Node {
            #rerun
            use ::vue_rs_dom::El;
            // Each component owns a scope: its effects and provided contexts are
            // scoped to this subtree.
            ::vue_rs_reactive::run_in_child_scope(move || {
                #(#body)*
                #template
            })
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
    slot_defs: &[(Ident, TokenStream)],
) -> (TokenStream, TokenStream) {
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
        let setter = Ident::new(&format!("with_{field}"), Span::call_site());
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
fn gen_props_builder(item: &syn::ItemStruct) -> Result<TokenStream, String> {
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
        .map(|(i, _)| Ident::new(&format!("__M{i}"), Span::call_site()))
        .collect();
    let k = marker_params.len();
    let unset = quote! { ::vue_rs_dom::builder::Unset };
    let set = quote! { ::vue_rs_dom::builder::Set };

    // Angle-bracket a marker list, or nothing when there are no required fields.
    let generics = |markers: Vec<TokenStream>| {
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
            let in_markers: Vec<TokenStream> = marker_params
                .iter()
                .enumerate()
                .map(|(j, m)| if j == pos { unset.clone() } else { quote! { #m } })
                .collect();
            let out_markers: Vec<TokenStream> = marker_params
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
fn slot_fields(slots: &syn::ItemStruct) -> Vec<(String, TokenStream)> {
    let syn::Fields::Named(fields) = &slots.fields else {
        return Vec::new();
    };
    let mut out: Vec<(String, TokenStream)> = fields
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
