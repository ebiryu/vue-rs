//! Contract: split a `.vrs` single-file component into its blocks.

use vue_rs_compiler::split_sfc;

const FULL: &str = r#"<template>
  <button @click="increment()">count is {{ count.get() }}</button>
</template>

<script lang="rust">
use vue_rs_reactive::signal;

let count = signal(0);
</script>

<style scoped>
button { color: red; }
</style>
"#;

#[test]
fn splits_all_three_blocks() {
    let sfc = split_sfc(FULL).expect("valid sfc");
    assert_eq!(
        sfc.template,
        r#"<button @click="increment()">count is {{ count.get() }}</button>"#
    );
    assert_eq!(
        sfc.script.as_deref(),
        Some("use vue_rs_reactive::signal;\n\nlet count = signal(0);")
    );
    assert_eq!(sfc.style.as_deref(), Some("button { color: red; }"));
}

#[test]
fn template_with_nested_tags_is_captured_whole() {
    let src = "<template><ul><li>a</li><li>b</li></ul></template>";
    let sfc = split_sfc(src).expect("valid sfc");
    assert_eq!(sfc.template, "<ul><li>a</li><li>b</li></ul>");
}

#[test]
fn script_and_style_are_optional() {
    let sfc = split_sfc("<template><p>hi</p></template>").expect("valid sfc");
    assert_eq!(sfc.template, "<p>hi</p>");
    assert_eq!(sfc.script, None);
    assert_eq!(sfc.style, None);
}

#[test]
fn missing_template_is_an_error() {
    assert!(split_sfc("<script lang=\"rust\">let x = 1;</script>").is_err());
}

#[test]
fn unterminated_template_is_an_error() {
    assert!(split_sfc("<template><p>hi</p>").is_err());
}
