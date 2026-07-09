use vue_rs_build::{component_name, compile_source};

#[test]
fn pascal_case_names() {
    assert_eq!(component_name("counter"), "Counter");
    assert_eq!(component_name("todo_app"), "TodoApp");
}

#[test]
fn compiles_a_minimal_component_to_rust_source() {
    let src = r#"
<template><button @click="count.set(count.get()+1)">{{ count.get() }}</button></template>
<script lang="rust">
use vue_rs_reactive::signal;
let count = signal(0);
</script>
"#;
    let out = compile_source("counter", src, "src/counter.vrs").unwrap();
    assert!(out.contains("pub fn Counter"), "output names the component fn:\n{out}");
}

#[test]
fn reports_compile_errors_with_the_file_path() {
    // A malformed template surfaces an error tagged with the seed path.
    let err = compile_source("bad", "<template><div></template>", "src/bad.vrs").unwrap_err();
    assert!(err.to_string().contains("src/bad.vrs"), "error mentions the file: {err}");
}
