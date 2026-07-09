use proc_macro2::{Ident, Span};
use vue_rs_compiler::compile_component;

#[test]
fn compiles_a_minimal_component() {
    let src = r#"
<template><button @click="count.set(count.get()+1)">{{ count.get() }}</button></template>
<script lang="rust">
use vue_rs_reactive::signal;
let count = signal(0);
</script>
"#;
    let name = Ident::new("Counter", Span::call_site());
    let tokens = compile_component(&name, src, "src/counter.vrs", None).unwrap();
    // Parses as valid Rust and defines `pub fn Counter`.
    let file: syn::File = syn::parse2(tokens).expect("output is valid Rust");
    let has_fn = file
        .items
        .iter()
        .any(|i| matches!(i, syn::Item::Fn(f) if f.sig.ident == "Counter"));
    assert!(has_fn, "expected a `pub fn Counter` in the output");
}

#[test]
fn omits_rerun_trigger_when_path_is_none() {
    let name = Ident::new("Counter", Span::call_site());
    let tokens = compile_component(&name, "<template><p>hi</p></template>", "x", None).unwrap();
    assert!(
        !tokens.to_string().contains("include_bytes"),
        "no rebuild trigger without a path"
    );
}
