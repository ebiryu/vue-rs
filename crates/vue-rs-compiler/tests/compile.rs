//! Contract: a Vue-style template string compiles to `El`-builder Rust code.
//!
//! The generated code references `__backend` (the backend instance, bound by the
//! component macro) and the user's reactive bindings via their Rust expressions.
//! Interpolations carry full Rust exprs (e.g. `{{ count.get() }}`); the `.get()`
//! sugar is a later authoring-layer concern.

use quote::quote;
use vue_rs_compiler::compile_template;

#[track_caller]
fn compiles_to(template: &str, expected: proc_macro2::TokenStream) {
    let out = compile_template(template).expect("template should compile");
    assert_eq!(out.to_string(), expected.to_string());
}

#[test]
fn static_element_with_text() {
    compiles_to(
        "<button>count is 0</button>",
        quote! { El::new(__backend.clone(), "button").text("count is 0").finish() },
    );
}

#[test]
fn static_attributes() {
    compiles_to(
        r#"<div id="app" class="root"></div>"#,
        quote! { El::new(__backend.clone(), "div").attr("id", "app").attr("class", "root").finish() },
    );
}

#[test]
fn interpolation_becomes_dyn_text() {
    compiles_to(
        "<p>{{ count.get() }}</p>",
        quote! { El::new(__backend.clone(), "p").dyn_text(move || (count.get()).to_string()).finish() },
    );
}

#[test]
fn mixed_static_text_and_interpolation() {
    compiles_to(
        "<button>count is {{ count.get() }}</button>",
        quote! {
            El::new(__backend.clone(), "button")
                .text("count is ")
                .dyn_text(move || (count.get()).to_string())
                .finish()
        },
    );
}

#[test]
fn bound_attribute_becomes_dyn_attr() {
    compiles_to(
        r#"<div :class="cls()"></div>"#,
        quote! {
            El::new(__backend.clone(), "div")
                .dyn_attr("class", move || (cls()).to_string())
                .finish()
        },
    );
}

#[test]
fn event_handler_becomes_on() {
    compiles_to(
        r#"<button @click="count.set(count.get() + 1)">x</button>"#,
        quote! {
            El::new(__backend.clone(), "button")
                .on("click", move || { count.set(count.get() + 1) })
                .text("x")
                .finish()
        },
    );
}

#[test]
fn self_closing_element() {
    compiles_to(
        r#"<input :value="v()" />"#,
        quote! {
            El::new(__backend.clone(), "input")
                .dyn_attr("value", move || (v()).to_string())
                .finish()
        },
    );
}

#[test]
fn nested_elements() {
    compiles_to(
        "<ul><li>a</li></ul>",
        quote! {
            El::new(__backend.clone(), "ul")
                .child(El::new(__backend.clone(), "li").text("a").finish())
                .finish()
        },
    );
}

#[test]
fn whitespace_between_elements_is_dropped() {
    compiles_to(
        "<ul>\n  <li>a</li>\n  <li>b</li>\n</ul>",
        quote! {
            El::new(__backend.clone(), "ul")
                .child(El::new(__backend.clone(), "li").text("a").finish())
                .child(El::new(__backend.clone(), "li").text("b").finish())
                .finish()
        },
    );
}

#[test]
fn error_on_multiple_root_elements() {
    assert!(compile_template("<p>a</p><p>b</p>").is_err());
}

#[test]
fn error_on_mismatched_closing_tag() {
    assert!(compile_template("<p>a</div>").is_err());
}

#[test]
fn error_on_invalid_expression() {
    assert!(compile_template("<p>{{ count.get( }}</p>").is_err());
}
