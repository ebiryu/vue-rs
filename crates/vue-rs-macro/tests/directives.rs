//! End-to-end: `v-if` / `v-else` / `v-for` / `v-model` compile through `view!`
//! and stay reactive on `MockDom`.

use vue_rs_dom::{El, MockDom, MockEvent, RawHtml};
use vue_rs_macro::view;
use vue_rs_reactive::signal;

#[test]
fn v_if_mounts_and_unmounts() {
    let dom = MockDom::new();
    let show = signal(false);

    let node = view!(
        dom.clone(),
        r#"<div><p v-if="show.get()">visible</p></div>"#
    );

    assert_eq!(dom.to_html(node), "<div></div>");
    show.set(true);
    assert_eq!(dom.to_html(node), "<div><p>visible</p></div>");
    show.set(false);
    assert_eq!(dom.to_html(node), "<div></div>");
}

#[test]
fn v_if_v_else_swaps() {
    let dom = MockDom::new();
    let on = signal(true);

    let node = view!(
        dom.clone(),
        r#"<div><p v-if="on.get()">yes</p><span v-else>no</span></div>"#
    );

    assert_eq!(dom.to_html(node), "<div><p>yes</p></div>");
    on.set(false);
    assert_eq!(dom.to_html(node), "<div><span>no</span></div>");
}

#[test]
fn v_if_else_if_else_chain() {
    let dom = MockDom::new();
    let n = signal(0);

    let node = view!(
        dom.clone(),
        r#"<div><p v-if="n.get() == 0">zero</p><p v-else-if="n.get() == 1">one</p><span v-else>many</span></div>"#
    );

    assert_eq!(dom.to_html(node), "<div><p>zero</p></div>");
    n.set(1);
    assert_eq!(dom.to_html(node), "<div><p>one</p></div>");
    n.set(5);
    assert_eq!(dom.to_html(node), "<div><span>many</span></div>");
    n.set(0);
    assert_eq!(dom.to_html(node), "<div><p>zero</p></div>");
}

#[test]
fn v_if_else_if_without_trailing_else() {
    let dom = MockDom::new();
    let n = signal(0);

    let node = view!(
        dom.clone(),
        r#"<div><p v-if="n.get() == 0">zero</p><p v-else-if="n.get() == 1">one</p></div>"#
    );

    assert_eq!(dom.to_html(node), "<div><p>zero</p></div>");
    n.set(1);
    assert_eq!(dom.to_html(node), "<div><p>one</p></div>");
    n.set(2);
    assert_eq!(dom.to_html(node), "<div></div>");
}

#[test]
fn v_for_keyed_list() {
    let dom = MockDom::new();
    let items = signal(vec![1, 2, 3]);

    let node = view!(
        dom.clone(),
        r#"<ul><li v-for="n in items.get()" :key="n">{{ n.to_string() }}</li></ul>"#
    );

    assert_eq!(dom.to_html(node), "<ul><li>1</li><li>2</li><li>3</li></ul>");

    items.set(vec![3, 1]);
    assert_eq!(dom.to_html(node), "<ul><li>3</li><li>1</li></ul>");
}

#[test]
fn v_for_with_index_binding() {
    let dom = MockDom::new();
    let items = signal(vec!["a".to_string(), "b".to_string(), "c".to_string()]);

    let node = view!(
        dom.clone(),
        r#"<ul><li v-for="(item, i) in items.get()" :key="item">{{ format!("{}:{}", i, item) }}</li></ul>"#
    );

    assert_eq!(
        dom.to_html(node),
        "<ul><li>0:a</li><li>1:b</li><li>2:c</li></ul>"
    );
}

#[test]
fn v_for_row_click_fires_handler() {
    let dom = MockDom::new();
    let items = signal(vec![10, 20]);
    let clicked = signal(0);
    let _node = view!(
        dom.clone(),
        r#"<ul><li v-for="n in items.get()" :key="n"><button @click="clicked.set(n)">{{ n.to_string() }}</button></li></ul>"#
    );
    let button = dom.find("button").expect("a row button");
    dom.dispatch(button, "click");
    assert_eq!(clicked.get(), 10);
}

#[test]
fn event_modifiers_apply_options_and_run_handler() {
    let dom = MockDom::new();
    let saved = signal(0);
    let node = view!(
        dom.clone(),
        r#"<form @submit.prevent="saved.set(saved.get() + 1)">x</form>"#
    );
    let outcome = dom.dispatch(node, "submit");
    assert_eq!(saved.get(), 1, "handler runs");
    assert!(outcome.default_prevented, ".prevent calls preventDefault");
}

#[test]
fn event_once_modifier_fires_a_single_time() {
    let dom = MockDom::new();
    let n = signal(0);
    let node = view!(
        dom.clone(),
        r#"<button @click.once="n.set(n.get() + 1)">x</button>"#
    );
    dom.dispatch(node, "click");
    dom.dispatch(node, "click");
    assert_eq!(n.get(), 1);
}

#[test]
fn key_modifier_runs_handler_only_for_matching_key() {
    let dom = MockDom::new();
    let submitted = signal(0);
    let node = view!(
        dom.clone(),
        r#"<input @keyup.enter="submitted.set(submitted.get() + 1)" />"#
    );
    dom.dispatch_event(
        node,
        "keyup",
        MockEvent {
            key: Some("a".to_string()),
            ..Default::default()
        },
    );
    assert_eq!(submitted.get(), 0, "other keys ignored");
    dom.dispatch_event(
        node,
        "keyup",
        MockEvent {
            key: Some("Enter".to_string()),
            ..Default::default()
        },
    );
    assert_eq!(submitted.get(), 1, "Enter fires the handler");
}

#[test]
fn system_modifier_runs_handler_only_when_modifier_is_held() {
    let dom = MockDom::new();
    let saved = signal(0);
    let node = view!(
        dom.clone(),
        r#"<input @keyup.ctrl.enter="saved.set(saved.get() + 1)" />"#
    );
    // Enter without ctrl: the system-modifier guard skips the handler.
    dom.dispatch_event(
        node,
        "keyup",
        MockEvent {
            key: Some("Enter".to_string()),
            ..Default::default()
        },
    );
    assert_eq!(saved.get(), 0, "Enter alone is ignored without ctrl");
    // ctrl+Enter: both guards pass.
    dom.dispatch_event(
        node,
        "keyup",
        MockEvent {
            key: Some("Enter".to_string()),
            ctrl: true,
            ..Default::default()
        },
    );
    assert_eq!(saved.get(), 1, "ctrl+Enter fires the handler");
}

#[test]
fn exact_modifier_runs_handler_only_for_the_exact_modifier_set() {
    let dom = MockDom::new();
    let hits = signal(0);
    let node = view!(
        dom.clone(),
        r#"<button @click.ctrl.exact="hits.set(hits.get() + 1)">x</button>"#
    );
    // ctrl plus an extra shift: `.exact` rejects the surplus modifier.
    dom.dispatch_event(
        node,
        "click",
        MockEvent {
            ctrl: true,
            shift: true,
            ..Default::default()
        },
    );
    assert_eq!(hits.get(), 0, "extra modifier rejected by .exact");
    dom.dispatch_event(
        node,
        "click",
        MockEvent {
            ctrl: true,
            ..Default::default()
        },
    );
    assert_eq!(hits.get(), 1, "exactly ctrl fires the handler");
}

#[test]
fn self_modifier_ignores_events_from_descendants() {
    let dom = MockDom::new();
    let hits = signal(0);
    let node = view!(
        dom.clone(),
        r#"<div @click.self="hits.set(hits.get() + 1)"><span>inner</span></div>"#
    );
    let inner = dom.find("span").expect("inner span");
    dom.dispatch_event(
        node,
        "click",
        MockEvent {
            target: Some(inner),
            ..Default::default()
        },
    );
    assert_eq!(hits.get(), 0, "click bubbling from a child is ignored");
    dom.dispatch(node, "click");
    assert_eq!(hits.get(), 1, "click on the element itself fires");
}

#[test]
fn v_show_toggles_display_style() {
    let dom = MockDom::new();
    let visible = signal(true);

    let node = view!(dom.clone(), r#"<p v-show="visible.get()">hi</p>"#);

    // Shown: no `display: none`. Hidden: the element stays mounted but is
    // collapsed via inline `display: none` (unlike `v-if`, which unmounts).
    assert_eq!(dom.to_html(node), r#"<p style="">hi</p>"#);
    visible.set(false);
    assert_eq!(dom.to_html(node), r#"<p style="display: none">hi</p>"#);
    visible.set(true);
    assert_eq!(dom.to_html(node), r#"<p style="">hi</p>"#);
}

#[test]
fn v_show_merges_with_static_style() {
    let dom = MockDom::new();
    let visible = signal(true);

    let node = view!(
        dom.clone(),
        r#"<p v-show="visible.get()" style="color: red">hi</p>"#
    );

    // The element's own `style` is preserved; `v-show` only appends/removes the
    // `display: none` declaration, instead of clobbering the whole attribute.
    assert_eq!(dom.to_html(node), r#"<p style="color: red">hi</p>"#);
    visible.set(false);
    assert_eq!(
        dom.to_html(node),
        r#"<p style="color: red; display: none">hi</p>"#
    );
    visible.set(true);
    assert_eq!(dom.to_html(node), r#"<p style="color: red">hi</p>"#);
}

#[test]
fn v_show_merges_with_dynamic_style() {
    let dom = MockDom::new();
    let visible = signal(true);
    let color = signal(String::from("color: red"));

    let node = view!(
        dom.clone(),
        r#"<p v-show="visible.get()" :style="color.get()">hi</p>"#
    );

    // The `:style` value is preserved and stays reactive; `v-show` only
    // appends/removes `display: none` on top of it, within a single effect.
    assert_eq!(dom.to_html(node), r#"<p style="color: red">hi</p>"#);
    visible.set(false);
    assert_eq!(
        dom.to_html(node),
        r#"<p style="color: red; display: none">hi</p>"#
    );
    color.set(String::from("color: blue"));
    assert_eq!(
        dom.to_html(node),
        r#"<p style="color: blue; display: none">hi</p>"#
    );
    visible.set(true);
    assert_eq!(dom.to_html(node), r#"<p style="color: blue">hi</p>"#);
}

#[test]
fn class_object_syntax_toggles_classes_reactively() {
    let dom = MockDom::new();
    let active = signal(true);
    let error = signal(false);

    let node = view!(
        dom.clone(),
        r#"<div :class="{ active: active.get(), 'text-danger': error.get() }"></div>"#
    );

    assert_eq!(dom.to_html(node), r#"<div class="active"></div>"#);
    error.set(true);
    assert_eq!(
        dom.to_html(node),
        r#"<div class="active text-danger"></div>"#
    );
    active.set(false);
    assert_eq!(dom.to_html(node), r#"<div class="text-danger"></div>"#);
}

#[test]
fn class_array_syntax_joins_classes_reactively() {
    let dom = MockDom::new();
    let extra = signal(String::from("b"));

    let node = view!(dom.clone(), r#"<div :class="[\"a\", extra.get()]"></div>"#);

    assert_eq!(dom.to_html(node), r#"<div class="a b"></div>"#);
    // An empty fragment leaves no stray separator.
    extra.set(String::new());
    assert_eq!(dom.to_html(node), r#"<div class="a"></div>"#);
}

#[test]
fn static_class_merges_with_dynamic_class() {
    let dom = MockDom::new();
    let active = signal(false);

    let node = view!(
        dom.clone(),
        r#"<div class="card" :class="{ active: active.get() }"></div>"#
    );

    // The static `class` is always present; the dynamic part merges onto it.
    assert_eq!(dom.to_html(node), r#"<div class="card"></div>"#);
    active.set(true);
    assert_eq!(dom.to_html(node), r#"<div class="card active"></div>"#);
}

#[test]
fn style_object_syntax_builds_style_reactively() {
    let dom = MockDom::new();
    let color = signal(String::from("red"));
    let size = signal(String::from("14px"));

    let node = view!(
        dom.clone(),
        r#"<div :style="{ color: color.get(), fontSize: size.get() }"></div>"#
    );

    // A bare camelCase key (`fontSize`) renders as kebab-case (`font-size`).
    assert_eq!(
        dom.to_html(node),
        r#"<div style="color: red; font-size: 14px"></div>"#
    );
    color.set(String::from("blue"));
    assert_eq!(
        dom.to_html(node),
        r#"<div style="color: blue; font-size: 14px"></div>"#
    );
    // An empty value drops the declaration without a stray separator.
    size.set(String::new());
    assert_eq!(dom.to_html(node), r#"<div style="color: blue"></div>"#);
}

#[test]
fn style_array_syntax_joins_styles_reactively() {
    let dom = MockDom::new();
    let extra = signal(String::from("color: blue"));

    let node = view!(dom.clone(), r#"<div :style="[\"margin: 0\", extra.get()]"></div>"#);

    assert_eq!(
        dom.to_html(node),
        r#"<div style="margin: 0; color: blue"></div>"#
    );
    // An empty fragment leaves no stray separator.
    extra.set(String::new());
    assert_eq!(dom.to_html(node), r#"<div style="margin: 0"></div>"#);
}

#[test]
fn static_style_merges_with_dynamic_style() {
    let dom = MockDom::new();
    let color = signal(String::from("red"));

    let node = view!(
        dom.clone(),
        r#"<div style="margin: 0" :style="{ color: color.get() }"></div>"#
    );

    // The static `style` is always present; the dynamic part merges onto it.
    assert_eq!(
        dom.to_html(node),
        r#"<div style="margin: 0; color: red"></div>"#
    );
    color.set(String::from("blue"));
    assert_eq!(
        dom.to_html(node),
        r#"<div style="margin: 0; color: blue"></div>"#
    );
}

#[test]
fn v_model_two_way_binding() {
    let dom = MockDom::new();
    let text = signal(String::from("hi"));

    let node = view!(dom.clone(), r#"<input v-model="text" />"#);

    assert_eq!(dom.to_html(node), r#"<input value="hi"></input>"#);

    // Simulate typing: the input event carries the new value.
    dom.dispatch_value(node, "input", "world");
    assert_eq!(dom.to_html(node), r#"<input value="world"></input>"#);
    assert_eq!(text.get(), "world");
}

#[test]
fn v_model_lazy_syncs_on_change_not_input() {
    let dom = MockDom::new();
    let text = signal(String::from("hi"));

    let node = view!(dom.clone(), r#"<input v-model.lazy="text" />"#);

    // `input` events are ignored with `.lazy`; only `change` syncs.
    dom.dispatch_value(node, "input", "typing");
    assert_eq!(text.get(), "hi");

    dom.dispatch_value(node, "change", "committed");
    assert_eq!(text.get(), "committed");
    assert_eq!(dom.to_html(node), r#"<input value="committed"></input>"#);
}

#[test]
fn v_model_trim_strips_surrounding_whitespace() {
    let dom = MockDom::new();
    let text = signal(String::new());

    let node = view!(dom.clone(), r#"<input v-model.trim="text" />"#);

    dom.dispatch_value(node, "input", "  spaced  ");
    assert_eq!(text.get(), "spaced");
}

#[test]
fn v_model_number_parses_into_numeric_model() {
    let dom = MockDom::new();
    let count = signal(0_i32);

    let node = view!(dom.clone(), r#"<input v-model.number="count" />"#);

    dom.dispatch_value(node, "input", "42");
    assert_eq!(count.get(), 42);

    // Invalid input keeps the current value rather than resetting it.
    dom.dispatch_value(node, "input", "abc");
    assert_eq!(count.get(), 42);
}

#[test]
fn v_html_renders_raw_markup_reactively() {
    let dom = MockDom::new();
    let body = signal(String::from("<b>bold</b>"));

    // `v-html` requires the expression to yield `RawHtml`, so the danger is
    // visible at the call site (akin to React's `dangerouslySetInnerHTML`).
    let node = view!(
        dom.clone(),
        r#"<div v-html="RawHtml::dangerously_from_html(body.get())"></div>"#
    );

    assert_eq!(dom.to_html(node), "<div><b>bold</b></div>");
    body.set(String::from("<i>italic</i>"));
    assert_eq!(dom.to_html(node), "<div><i>italic</i></div>");
}

#[test]
fn v_text_renders_text_content_reactively() {
    let dom = MockDom::new();
    let msg = signal(String::from("hello"));

    // `v-text` sets the element's text content; unlike `v-html` the value is
    // escaped, and template children are ignored.
    let node = view!(dom.clone(), r#"<span v-text="msg.get()">ignored</span>"#);

    assert_eq!(dom.to_html(node), "<span>hello</span>");
    msg.set(String::from("<world>"));
    assert_eq!(dom.to_html(node), "<span>&lt;world&gt;</span>");
}
