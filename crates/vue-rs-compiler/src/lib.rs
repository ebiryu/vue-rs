//! Compiles vue-rs templates into Rust `El`-builder code.
//!
//! [`compile_template`] takes the contents of a `<template>` and returns a
//! `TokenStream` building the view. The generated code references `__backend`
//! (the backend instance) and the user's reactive bindings via the Rust
//! expressions written inside `{{ }}`, `:attr`, and `@event`.

use std::fmt;

mod codegen;
mod parser;
mod props;
mod script;
mod sfc;
mod style;

pub use props::check_prop_fields;
pub use script::rewrite_script_sugar;
pub use sfc::{split_sfc, Sfc};
pub use style::{scope_css, scope_id};

/// A template compilation failure, with a human-readable message.
#[derive(Debug)]
pub struct CompileError(pub String);

impl fmt::Display for CompileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for CompileError {}

/// Compile a template string into `El`-builder Rust code.
pub fn compile_template(template: &str) -> Result<proc_macro2::TokenStream, CompileError> {
    let nodes = parser::parse(template).map_err(CompileError)?;
    codegen::Codegen::new(None).root(&nodes).map_err(CompileError)
}

/// Like [`compile_template`], but tags every element with a `data-v-<scope_id>`
/// marker attribute so scoped CSS (see [`scope_css`]) can target it.
pub fn compile_template_scoped(
    template: &str,
    scope_id: &str,
) -> Result<proc_macro2::TokenStream, CompileError> {
    let nodes = parser::parse(template).map_err(CompileError)?;
    codegen::Codegen::new(Some(scope_id.to_string()))
        .root(&nodes)
        .map_err(CompileError)
}

/// The result of compiling a component's `<template>`.
pub struct CompiledComponent {
    /// The `El`-builder code for the template.
    pub tokens: proc_macro2::TokenStream,
    /// Every `<slot>` the template uses, as `(name, scoped)` pairs. The caller
    /// generates the component's `NameSlots` struct from this: one
    /// `Option<SlotFn<B, _>>` field per slot (scoped slots use their declared
    /// payload type, plain slots use `()`).
    pub slots: Vec<(String, bool)>,
}

/// Compile a component's `<template>`, given the payload type of each scoped
/// slot (keyed by slot name, taken from the component's declared `NameSlots`
/// fields) and an optional scoped-CSS marker id. A `<slot :field="x">` builds
/// the named payload struct; the parent supplies it through the `NameSlots`
/// struct's `with_<name>` builder.
pub fn compile_component_template(
    template: &str,
    scope_id: Option<&str>,
    slot_payloads: Vec<(String, proc_macro2::TokenStream)>,
) -> Result<CompiledComponent, CompileError> {
    let nodes = parser::parse(template).map_err(CompileError)?;
    let codegen =
        codegen::Codegen::with_slot_payloads(scope_id.map(str::to_string), slot_payloads.into_iter().collect());
    let tokens = codegen.root(&nodes).map_err(CompileError)?;
    Ok(CompiledComponent {
        tokens,
        slots: codegen.slots(),
    })
}
