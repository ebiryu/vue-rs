//! Contract for scoped-style helpers and scoped template codegen.

use quote::quote;
use vue_rs_compiler::{compile_template_scoped, scope_css, scope_id};

#[test]
fn scope_css_appends_marker_to_selector() {
    assert_eq!(
        scope_css("button { color: red; }", "abc123"),
        "button[data-v-abc123]{ color: red; }"
    );
}

#[test]
fn scope_css_handles_multiple_selectors() {
    assert_eq!(
        scope_css(".a, .b { x: 1; }", "id"),
        ".a[data-v-id], .b[data-v-id]{ x: 1; }"
    );
}

#[test]
fn scope_css_inserts_marker_before_pseudo_class() {
    assert_eq!(
        scope_css("button:hover { color: red; }", "abc"),
        "button[data-v-abc]:hover{ color: red; }"
    );
}

#[test]
fn scope_css_inserts_marker_before_pseudo_element() {
    assert_eq!(
        scope_css(".foo::before { content: x; }", "id"),
        ".foo[data-v-id]::before{ content: x; }"
    );
}

#[test]
fn scope_css_scopes_last_compound_in_descendant_with_pseudo() {
    assert_eq!(
        scope_css(".list li:first-child { x: 1; }", "id"),
        ".list li[data-v-id]:first-child{ x: 1; }"
    );
}

#[test]
fn scope_css_ignores_combinator_chars_inside_pseudo_args() {
    assert_eq!(
        scope_css("li:nth-child(2n+1) { x: 1; }", "id"),
        "li[data-v-id]:nth-child(2n+1){ x: 1; }"
    );
}

#[test]
fn scope_id_is_stable() {
    assert_eq!(scope_id("src/foo.vrs"), scope_id("src/foo.vrs"));
    assert_ne!(scope_id("a"), scope_id("b"));
}

#[test]
fn scoped_template_marks_each_element() {
    let out = compile_template_scoped("<div><p>hi</p></div>", "abc").expect("compile");
    let expected = quote! {
        El::new(__backend.clone(), "div")
            .attr("data-v-abc", "")
            .child(
                El::new(__backend.clone(), "p")
                    .attr("data-v-abc", "")
                    .text("hi")
                    .finish()
            )
            .finish()
    };
    assert_eq!(out.to_string(), expected.to_string());
}
