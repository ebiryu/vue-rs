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
fn v_text_becomes_dyn_text() {
    // `v-text` is sugar for a single `{{ }}` child: it sets the element's text
    // content from a reactive expression.
    compiles_to(
        r#"<span v-text="msg.get()"></span>"#,
        quote! {
            El::new(__backend.clone(), "span")
                .dyn_text(move || (msg.get()).to_string())
                .finish()
        },
    );
}

#[test]
fn v_text_ignores_template_children() {
    // Like Vue, `v-text` owns the element's content, so template children are
    // dropped.
    compiles_to(
        r#"<span v-text="msg.get()">ignored</span>"#,
        quote! {
            El::new(__backend.clone(), "span")
                .dyn_text(move || (msg.get()).to_string())
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
fn event_with_modifiers_becomes_on_opts() {
    compiles_to(
        r#"<form @submit.prevent.stop="save()">x</form>"#,
        quote! {
            El::new(__backend.clone(), "form")
                .on_opts(
                    "submit",
                    ::vue_rs_dom::EventOptions {
                        prevent_default: true,
                        stop_propagation: true,
                        ..::core::default::Default::default()
                    },
                    move || { save() }
                )
                .text("x")
                .finish()
        },
    );
}

#[test]
fn event_with_once_modifier_becomes_on_opts() {
    compiles_to(
        r#"<button @click.once="go()">x</button>"#,
        quote! {
            El::new(__backend.clone(), "button")
                .on_opts(
                    "click",
                    ::vue_rs_dom::EventOptions {
                        once: true,
                        ..::core::default::Default::default()
                    },
                    move || { go() }
                )
                .text("x")
                .finish()
        },
    );
}

#[test]
fn event_self_capture_passive_modifiers_become_options() {
    compiles_to(
        r#"<div @scroll.self.capture.passive="onScroll()">x</div>"#,
        quote! {
            El::new(__backend.clone(), "div")
                .on_opts(
                    "scroll",
                    ::vue_rs_dom::EventOptions {
                        self_only: true,
                        capture: true,
                        passive: true,
                        ..::core::default::Default::default()
                    },
                    move || { onScroll() }
                )
                .text("x")
                .finish()
        },
    );
}

#[test]
fn key_modifier_on_keyboard_event_becomes_key_filter() {
    compiles_to(
        r#"<input @keyup.enter="submit()" />"#,
        quote! {
            El::new(__backend.clone(), "input")
                .on_opts(
                    "keyup",
                    ::vue_rs_dom::EventOptions {
                        keys: &["Enter"],
                        ..::core::default::Default::default()
                    },
                    move || { submit() }
                )
                .finish()
        },
    );
}

#[test]
fn arrow_key_alias_maps_to_event_key_on_keyboard_event() {
    compiles_to(
        r#"<input @keydown.left="prev()" />"#,
        quote! {
            El::new(__backend.clone(), "input")
                .on_opts(
                    "keydown",
                    ::vue_rs_dom::EventOptions {
                        keys: &["ArrowLeft"],
                        ..::core::default::Default::default()
                    },
                    move || { prev() }
                )
                .finish()
        },
    );
}

#[test]
fn mouse_button_modifier_on_mouse_event_becomes_button_filter() {
    compiles_to(
        r#"<button @click.right="ctx()">x</button>"#,
        quote! {
            El::new(__backend.clone(), "button")
                .on_opts(
                    "click",
                    ::vue_rs_dom::EventOptions {
                        buttons: &[2u16],
                        ..::core::default::Default::default()
                    },
                    move || { ctx() }
                )
                .text("x")
                .finish()
        },
    );
}

#[test]
fn system_modifiers_become_options_on_any_event() {
    compiles_to(
        r#"<div @click.ctrl.shift="go()">x</div>"#,
        quote! {
            El::new(__backend.clone(), "div")
                .on_opts(
                    "click",
                    ::vue_rs_dom::EventOptions {
                        ctrl: true,
                        shift: true,
                        ..::core::default::Default::default()
                    },
                    move || { go() }
                )
                .text("x")
                .finish()
        },
    );
}

#[test]
fn exact_modifier_becomes_option() {
    compiles_to(
        r#"<div @click.ctrl.exact="go()">x</div>"#,
        quote! {
            El::new(__backend.clone(), "div")
                .on_opts(
                    "click",
                    ::vue_rs_dom::EventOptions {
                        ctrl: true,
                        exact: true,
                        ..::core::default::Default::default()
                    },
                    move || { go() }
                )
                .text("x")
                .finish()
        },
    );
}

#[test]
fn event_with_unknown_modifier_errors() {
    assert!(compile_template(r#"<button @click.bogus="go()">x</button>"#).is_err());
}

#[test]
fn key_modifier_on_mouse_event_errors() {
    // `.enter` is a key modifier but `click` is a mouse event: no key context.
    assert!(compile_template(r#"<button @click.enter="go()">x</button>"#).is_err());
}

#[test]
fn mouse_button_modifier_on_keyboard_event_errors() {
    // `.middle` is a mouse-button modifier; not valid on a keyboard event.
    assert!(compile_template(r#"<input @keyup.middle="go()" />"#).is_err());
}

#[test]
fn bound_attribute_value_keeps_escaped_quotes() {
    // A backslash-escaped quote inside the attribute value embeds the delimiter
    // quote into the Rust expression instead of terminating the value early.
    compiles_to(
        r#"<div :class="cls(\"x\")"></div>"#,
        quote! {
            El::new(__backend.clone(), "div")
                .dyn_attr("class", move || (cls("x")).to_string())
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
fn component_without_props() {
    // Every component call passes its slots struct, so a slot-bearing component
    // works even when the parent provides nothing.
    compiles_to(
        "<Child />",
        quote! { Child(__backend.clone(), ChildSlots::for_backend(&__backend)) },
    );
}

#[test]
fn component_with_props_and_event() {
    compiles_to(
        r#"<Child :value="count" @change="handler" />"#,
        quote! {
            Child(
                __backend.clone(),
                ChildProps {
                    value: count,
                    on_change: ::vue_rs_dom::Callback::new(handler)
                },
                ChildSlots::for_backend(&__backend)
            )
        },
    );
}

#[test]
fn component_nested_in_element() {
    compiles_to(
        r#"<div><Child :x="y" /></div>"#,
        quote! {
            El::new(__backend.clone(), "div")
                .child(Child(
                    __backend.clone(),
                    ChildProps { x: y },
                    ChildSlots::for_backend(&__backend)
                ))
                .finish()
        },
    );
}

#[test]
fn comment_inside_element_is_skipped() {
    compiles_to(
        "<div><!-- hello --></div>",
        quote! { El::new(__backend.clone(), "div").finish() },
    );
}

#[test]
fn comment_between_children_is_skipped() {
    compiles_to(
        "<ul><!-- first --><li>a</li><!-- second --><li>b</li></ul>",
        quote! {
            El::new(__backend.clone(), "ul")
                .child(El::new(__backend.clone(), "li").text("a").finish())
                .child(El::new(__backend.clone(), "li").text("b").finish())
                .finish()
        },
    );
}

#[test]
fn comment_around_root_is_skipped() {
    compiles_to(
        "<!-- leading --><p>a</p><!-- trailing -->",
        quote! { El::new(__backend.clone(), "p").text("a").finish() },
    );
}

#[test]
fn comment_with_markup_like_content_is_skipped() {
    compiles_to(
        "<p>x<!-- <span>not real</span> {{ nope }} -->y</p>",
        quote! { El::new(__backend.clone(), "p").text("xy").finish() },
    );
}

#[test]
fn error_on_unterminated_comment() {
    assert!(compile_template("<div><!-- oops</div>").is_err());
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
