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
