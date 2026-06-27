//! Compiles vue-rs templates into Rust `El`-builder code.
//!
//! [`compile_template`] takes the contents of a `<template>` and returns a
//! `TokenStream` building the view. The generated code references `__backend`
//! (the backend instance) and the user's reactive bindings via the Rust
//! expressions written inside `{{ }}`, `:attr`, and `@event`.

use std::fmt;

mod codegen;
mod parser;
mod sfc;

pub use sfc::{split_sfc, Sfc};

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
    codegen::codegen_root(&nodes).map_err(CompileError)
}
