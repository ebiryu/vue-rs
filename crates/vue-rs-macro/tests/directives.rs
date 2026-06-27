//! End-to-end: `v-if` / `v-else` / `v-for` / `v-model` compile through `view!`
//! and stay reactive on `MockDom`.

use vue_rs_dom::{El, MockDom};
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
