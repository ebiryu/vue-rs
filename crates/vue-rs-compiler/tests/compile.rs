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
fn template_ref_becomes_node_ref() {
    // `ref="el"` binds the element's node into the `el` template ref handle.
    compiles_to(
        r#"<input ref="el" />"#,
        quote! {
            El::new(__backend.clone(), "input")
                .node_ref(&el)
                .finish()
        },
    );
}

#[test]
fn template_ref_coexists_with_other_attributes() {
    compiles_to(
        r#"<input ref="field" :value="text()" />"#,
        quote! {
            El::new(__backend.clone(), "input")
                .node_ref(&field)
                .dyn_attr("value", move || (text()).to_string())
                .finish()
        },
    );
}

#[test]
fn template_ref_with_non_identifier_value_errors() {
    assert!(compile_template(r#"<input ref="a b" />"#).is_err());
}

#[test]
fn template_ref_on_component_errors() {
    // Component instance refs (`defineExpose`) are not supported yet.
    assert!(compile_template(r#"<Child ref="c" />"#).is_err());
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
fn class_object_syntax_becomes_class_list() {
    // `:class="{ name: cond }"` toggles each class by its condition.
    compiles_to(
        r#"<div :class="{ active: is_active(), 'text-danger': has_error() }"></div>"#,
        quote! {
            El::new(__backend.clone(), "div")
                .dyn_attr("class", move || ::vue_rs_dom::ClassList::new()
                    .push_if("active", is_active())
                    .push_if("text-danger", has_error())
                    .finish())
                .finish()
        },
    );
}

#[test]
fn class_array_syntax_becomes_class_list() {
    // `:class="[a, b]"` joins each element's class string.
    compiles_to(
        r#"<div :class="[base(), extra()]"></div>"#,
        quote! {
            El::new(__backend.clone(), "div")
                .dyn_attr("class", move || ::vue_rs_dom::ClassList::new()
                    .push(base())
                    .push(extra())
                    .finish())
                .finish()
        },
    );
}

#[test]
fn static_class_merges_with_object_class() {
    // A static `class` is the base the dynamic `:class` is merged onto.
    compiles_to(
        r#"<div class="card" :class="{ active: on() }"></div>"#,
        quote! {
            El::new(__backend.clone(), "div")
                .dyn_attr("class", move || ::vue_rs_dom::ClassList::new()
                    .push("card")
                    .push_if("active", on())
                    .finish())
                .finish()
        },
    );
}

#[test]
fn static_class_merges_with_plain_dynamic_class() {
    compiles_to(
        r#"<div class="card" :class="cls()"></div>"#,
        quote! {
            El::new(__backend.clone(), "div")
                .dyn_attr("class", move || ::vue_rs_dom::ClassList::new()
                    .push("card")
                    .push(cls())
                    .finish())
                .finish()
        },
    );
}

#[test]
fn plain_dynamic_style_stays_simple() {
    // A lone plain `:style` with no static sibling keeps the simple path.
    compiles_to(
        r#"<div :style="style_str()"></div>"#,
        quote! {
            El::new(__backend.clone(), "div")
                .dyn_attr("style", move || (style_str()).to_string())
                .finish()
        },
    );
}

#[test]
fn style_object_syntax_becomes_style_list() {
    // `:style="{ prop: value }"` builds each `prop: value` declaration. A quoted
    // key is taken verbatim; a bare camelCase key is converted to kebab-case.
    compiles_to(
        r#"<div :style="{ color: color(), 'font-size': size() }"></div>"#,
        quote! {
            El::new(__backend.clone(), "div")
                .dyn_attr("style", move || ::vue_rs_dom::StyleList::new()
                    .push_prop("color", (color()).to_string())
                    .push_prop("font-size", (size()).to_string())
                    .finish())
                .finish()
        },
    );
}

#[test]
fn style_object_camel_case_key_becomes_kebab() {
    compiles_to(
        r#"<div :style="{ fontSize: size() }"></div>"#,
        quote! {
            El::new(__backend.clone(), "div")
                .dyn_attr("style", move || ::vue_rs_dom::StyleList::new()
                    .push_prop("font-size", (size()).to_string())
                    .finish())
                .finish()
        },
    );
}

#[test]
fn style_array_syntax_becomes_style_list() {
    // `:style="[a, b]"` joins each element's declaration string.
    compiles_to(
        r#"<div :style="[base(), extra()]"></div>"#,
        quote! {
            El::new(__backend.clone(), "div")
                .dyn_attr("style", move || ::vue_rs_dom::StyleList::new()
                    .push(base())
                    .push(extra())
                    .finish())
                .finish()
        },
    );
}

#[test]
fn static_style_merges_with_object_style() {
    // A static `style` is the base the dynamic `:style` is merged onto.
    compiles_to(
        r#"<div style="margin: 0" :style="{ color: color() }"></div>"#,
        quote! {
            El::new(__backend.clone(), "div")
                .dyn_attr("style", move || ::vue_rs_dom::StyleList::new()
                    .push("margin: 0")
                    .push_prop("color", (color()).to_string())
                    .finish())
                .finish()
        },
    );
}

#[test]
fn static_style_merges_with_plain_dynamic_style() {
    compiles_to(
        r#"<div style="margin: 0" :style="style_str()"></div>"#,
        quote! {
            El::new(__backend.clone(), "div")
                .dyn_attr("style", move || ::vue_rs_dom::StyleList::new()
                    .push("margin: 0")
                    .push(style_str())
                    .finish())
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
fn v_model_lowers_to_value_attr_and_input_listener() {
    compiles_to(
        r#"<input v-model="text" />"#,
        quote! {
            El::new(__backend.clone(), "input")
                .dyn_attr("value", move || ((text).get()).to_string())
                .on_value("input", move |__value| (text).set(__value.to_string()))
                .finish()
        },
    );
}

#[test]
fn v_model_lazy_modifier_uses_change_event() {
    compiles_to(
        r#"<input v-model.lazy="text" />"#,
        quote! {
            El::new(__backend.clone(), "input")
                .dyn_attr("value", move || ((text).get()).to_string())
                .on_value("change", move |__value| (text).set(__value.to_string()))
                .finish()
        },
    );
}

#[test]
fn v_model_trim_modifier_trims_the_value() {
    compiles_to(
        r#"<input v-model.trim="text" />"#,
        quote! {
            El::new(__backend.clone(), "input")
                .dyn_attr("value", move || ((text).get()).to_string())
                .on_value("input", move |__value| (text).set(__value.trim().to_string()))
                .finish()
        },
    );
}

#[test]
fn v_model_number_modifier_parses_the_value() {
    compiles_to(
        r#"<input v-model.number="count" />"#,
        quote! {
            El::new(__backend.clone(), "input")
                .dyn_attr("value", move || ((count).get()).to_string())
                .on_value("input", move |__value| (count).set(
                    match __value.parse() {
                        ::core::result::Result::Ok(__n) => __n,
                        ::core::result::Result::Err(_) => (count).get(),
                    }
                ))
                .finish()
        },
    );
}

#[test]
fn v_model_trim_and_number_modifiers_combine() {
    compiles_to(
        r#"<input v-model.trim.number="count" />"#,
        quote! {
            El::new(__backend.clone(), "input")
                .dyn_attr("value", move || ((count).get()).to_string())
                .on_value("input", move |__value| (count).set(
                    match __value.trim().parse() {
                        ::core::result::Result::Ok(__n) => __n,
                        ::core::result::Result::Err(_) => (count).get(),
                    }
                ))
                .finish()
        },
    );
}

#[test]
fn v_model_unknown_modifier_errors() {
    assert!(compile_template(r#"<input v-model.bogus="text" />"#).is_err());
}

#[test]
fn v_model_on_checkbox_binds_checked_property() {
    compiles_to(
        r#"<input type="checkbox" v-model="done" />"#,
        quote! {
            El::new(__backend.clone(), "input")
                .attr("type", "checkbox")
                .dyn_bool_prop("checked", move || (done).get())
                .on_value("change", move |__value| (done).set(__value == "true"))
                .finish()
        },
    );
}

#[test]
fn v_model_on_checkbox_rejects_modifiers() {
    assert!(compile_template(r#"<input type="checkbox" v-model.number="done" />"#).is_err());
}

#[test]
fn v_model_on_radio_static_value_binds_checked_and_change() {
    // On `<input type="radio">`, `v-model` maps the model against the radio's
    // `value`: `checked` reflects equality, and selecting it writes that value
    // back. The `value` attribute is still rendered.
    compiles_to(
        r#"<input type="radio" value="a" v-model="picked" />"#,
        quote! {
            El::new(__backend.clone(), "input")
                .attr("type", "radio")
                .attr("value", "a")
                .dyn_bool_prop("checked", move || (picked).get() == "a")
                .on_value("change", move |_| (picked).set("a".to_string()))
                .finish()
        },
    );
}

#[test]
fn v_model_on_radio_dynamic_value_uses_the_expression() {
    compiles_to(
        r#"<input type="radio" :value="opt" v-model="picked" />"#,
        quote! {
            El::new(__backend.clone(), "input")
                .attr("type", "radio")
                .dyn_attr("value", move || (opt).to_string())
                .dyn_bool_prop("checked", move || (picked).get() == (opt))
                .on_value("change", move |_| (picked).set(opt))
                .finish()
        },
    );
}

#[test]
fn v_model_on_radio_rejects_modifiers() {
    assert!(
        compile_template(r#"<input type="radio" value="a" v-model.number="picked" />"#).is_err()
    );
}

#[test]
fn v_model_on_radio_without_value_errors() {
    assert!(compile_template(r#"<input type="radio" v-model="picked" />"#).is_err());
}

#[test]
fn v_model_on_textarea_binds_value_property() {
    // A `<textarea>` has no `value` attribute (its content is its children), so
    // `v-model` drives the `value` DOM property instead and syncs on `input`.
    compiles_to(
        r#"<textarea v-model="text"></textarea>"#,
        quote! {
            El::new(__backend.clone(), "textarea")
                .dyn_prop("value", move || ((text).get()).to_string())
                .on_value("input", move |__value| (text).set(__value.to_string()))
                .finish()
        },
    );
}

#[test]
fn v_model_on_textarea_honors_modifiers() {
    compiles_to(
        r#"<textarea v-model.trim="text"></textarea>"#,
        quote! {
            El::new(__backend.clone(), "textarea")
                .dyn_prop("value", move || ((text).get()).to_string())
                .on_value("input", move |__value| (text).set(__value.trim().to_string()))
                .finish()
        },
    );
}

#[test]
fn v_model_on_select_binds_value_property_and_syncs_on_change() {
    // A `<select>` has no `value` attribute either; `v-model` drives the `value`
    // DOM property and syncs on `change` (a selection is committed, not typed).
    compiles_to(
        r#"<select v-model="choice"></select>"#,
        quote! {
            El::new(__backend.clone(), "select")
                .dyn_prop("value", move || ((choice).get()).to_string())
                .on_value("change", move |__value| (choice).set(__value.to_string()))
                .finish()
        },
    );
}

#[test]
fn v_model_on_select_number_modifier_parses_the_value() {
    compiles_to(
        r#"<select v-model.number="choice"></select>"#,
        quote! {
            El::new(__backend.clone(), "select")
                .dyn_prop("value", move || ((choice).get()).to_string())
                .on_value("change", move |__value| (choice).set(
                    match __value.parse() {
                        ::core::result::Result::Ok(__n) => __n,
                        ::core::result::Result::Err(_) => (choice).get(),
                    }
                ))
                .finish()
        },
    );
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
fn dynamic_attribute_argument_becomes_dyn_attr_named() {
    // `:[name]="value"` computes the attribute name at runtime from the bracketed
    // Rust expression; both the name and value re-evaluate reactively.
    compiles_to(
        r#"<a :[attr]="url"></a>"#,
        quote! {
            El::new(__backend.clone(), "a")
                .dyn_attr_named(move || (attr).to_string(), move || (url).to_string())
                .finish()
        },
    );
}

#[test]
fn dynamic_event_argument_becomes_on_named() {
    // `@[event]="handler"` computes the event name at runtime; the listener is
    // re-attached when the name changes.
    compiles_to(
        r#"<button @[evt]="go()"></button>"#,
        quote! {
            El::new(__backend.clone(), "button")
                .on_named(move || (evt).to_string(), move || { go() })
                .finish()
        },
    );
}

#[test]
fn dynamic_attribute_argument_with_modifier_errors() {
    assert!(compile_template(r#"<a :[attr].camel="url"></a>"#).is_err());
}

#[test]
fn dynamic_event_argument_with_modifier_errors() {
    assert!(compile_template(r#"<button @[evt].stop="go()"></button>"#).is_err());
}

#[test]
fn dynamic_argument_on_component_errors() {
    assert!(compile_template(r#"<Child :[key]="v" />"#).is_err());
}

#[test]
fn bind_prop_modifier_becomes_dyn_prop() {
    // `:name.prop` sets a DOM *property* (via `set_property`) instead of an
    // attribute.
    compiles_to(
        r#"<input :value.prop="text()" />"#,
        quote! {
            El::new(__backend.clone(), "input")
                .dyn_prop("value", move || (text()).to_string())
                .finish()
        },
    );
}

#[test]
fn bind_attr_modifier_stays_dyn_attr() {
    // `:name.attr` forces an attribute (the default), spelled explicitly.
    compiles_to(
        r#"<div :id.attr="x()"></div>"#,
        quote! {
            El::new(__backend.clone(), "div")
                .dyn_attr("id", move || (x()).to_string())
                .finish()
        },
    );
}

#[test]
fn bind_camel_modifier_camelizes_attribute_name() {
    // `:view-box.camel` camelizes the bound name to `viewBox`.
    compiles_to(
        r#"<svg :view-box.camel="vb()"></svg>"#,
        quote! {
            El::new(__backend.clone(), "svg")
                .dyn_attr("viewBox", move || (vb()).to_string())
                .finish()
        },
    );
}

#[test]
fn bind_camel_and_prop_modifiers_combine() {
    compiles_to(
        r#"<x-el :my-prop.camel.prop="v()"></x-el>"#,
        quote! {
            El::new(__backend.clone(), "x-el")
                .dyn_prop("myProp", move || (v()).to_string())
                .finish()
        },
    );
}

#[test]
fn bind_unknown_modifier_errors() {
    assert!(compile_template(r#"<div :id.bogus="x()"></div>"#).is_err());
}

#[test]
fn bind_prop_and_attr_modifiers_conflict_errors() {
    assert!(compile_template(r#"<div :id.prop.attr="x()"></div>"#).is_err());
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
                    value: ::core::convert::Into::into(count),
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
                    ChildProps { x: ::core::convert::Into::into(y) },
                    ChildSlots::for_backend(&__backend)
                ))
                .finish()
        },
    );
}

#[test]
fn v_model_on_component_becomes_prop_down_and_emit_up() {
    // A component `v-model` lowers to a read-only `model_value` prop (value down)
    // plus an `on_update_model_value` callback that writes the source (up).
    compiles_to(
        r#"<Child v-model="name" />"#,
        quote! {
            Child(
                __backend.clone(),
                ChildProps {
                    model_value: ::core::convert::Into::into(name),
                    on_update_model_value: ::vue_rs_dom::Callback::new(move |__v| (name).set(__v))
                },
                ChildSlots::for_backend(&__backend)
            )
        },
    );
}

#[test]
fn v_model_arg_on_component_becomes_named_model_prop() {
    // `v-model:arg` binds the prop `arg` directly with an `on_update_<arg>` callback.
    compiles_to(
        r#"<Child v-model:title="heading" />"#,
        quote! {
            Child(
                __backend.clone(),
                ChildProps {
                    title: ::core::convert::Into::into(heading),
                    on_update_title: ::vue_rs_dom::Callback::new(move |__v| (heading).set(__v))
                },
                ChildSlots::for_backend(&__backend)
            )
        },
    );
}

#[test]
fn v_model_on_component_coexists_with_other_props() {
    compiles_to(
        r#"<Child :value="count" v-model="name" />"#,
        quote! {
            Child(
                __backend.clone(),
                ChildProps {
                    value: ::core::convert::Into::into(count),
                    model_value: ::core::convert::Into::into(name),
                    on_update_model_value: ::vue_rs_dom::Callback::new(move |__v| (name).set(__v))
                },
                ChildSlots::for_backend(&__backend)
            )
        },
    );
}

#[test]
fn v_model_on_component_with_modifier_errors() {
    let err = compile_template(r#"<Child v-model.number="n" />"#)
        .unwrap_err()
        .to_string();
    assert!(err.contains("modifier"), "unexpected error: {err}");
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
fn multiple_root_elements_become_a_fragment() {
    compiles_to(
        "<h1>Title</h1><p>Body</p>",
        quote! {
            ::vue_rs_dom::Backend::create_fragment(
                &__backend,
                ::std::vec![
                    El::new(__backend.clone(), "h1").text("Title").finish(),
                    El::new(__backend.clone(), "p").text("Body").finish()
                ],
            )
        },
    );
}

#[test]
fn root_fragment_carries_text_and_dynamic_text_members() {
    compiles_to(
        "<span>a</span>plain {{ x.get() }}",
        quote! {
            ::vue_rs_dom::Backend::create_fragment(
                &__backend,
                ::std::vec![
                    El::new(__backend.clone(), "span").text("a").finish(),
                    ::vue_rs_dom::Backend::create_text(&__backend, "plain "),
                    ::vue_rs_dom::dyn_text_node(&__backend, move || (x.get()).to_string())
                ],
            )
        },
    );
}

#[test]
fn single_root_element_does_not_become_a_fragment() {
    // The common case stays a bare element, with no fragment wrapping.
    compiles_to(
        "<p>hi</p>",
        quote! { El::new(__backend.clone(), "p").text("hi").finish() },
    );
}

#[test]
fn error_on_control_flow_at_template_root() {
    // Root-level control flow has no element to anchor against.
    assert!(compile_template(r#"<p v-if="x.get()">a</p><p>b</p>"#).is_err());
    assert!(compile_template(r#"<li v-for="i in xs" :key="i">{{ i }}</li><p>b</p>"#).is_err());
}

#[test]
fn error_on_mismatched_closing_tag() {
    assert!(compile_template("<p>a</div>").is_err());
}

#[test]
fn error_on_invalid_expression() {
    assert!(compile_template("<p>{{ count.get( }}</p>").is_err());
}
