//! End-to-end: a Vue-style template compiles via `view!`, renders on `MockDom`,
//! and stays reactive — clicking the button updates the interpolated text.

use vue_rs_dom::{El, MockDom};
use vue_rs_macro::view;
use vue_rs_reactive::signal;

#[test]
fn counter_renders_and_reacts_to_clicks() {
    let dom = MockDom::new();
    let count = signal(0);

    let node = view!(
        dom.clone(),
        r#"<button @click="count.set(count.get() + 1)">count is {{ count.get() }}</button>"#
    );

    assert_eq!(dom.to_html(node), "<button>count is 0</button>");

    dom.dispatch(node, "click");
    assert_eq!(dom.to_html(node), "<button>count is 1</button>");

    dom.dispatch(node, "click");
    assert_eq!(dom.to_html(node), "<button>count is 2</button>");
}

#[test]
fn nested_list_with_bindings() {
    let dom = MockDom::new();
    let label = signal(String::from("hello"));
    let css = signal(String::from("item"));

    let node = view!(
        dom.clone(),
        r#"<ul><li :class="css.get()">{{ label.get() }}</li></ul>"#
    );

    assert_eq!(dom.to_html(node), r#"<ul><li class="item">hello</li></ul>"#);

    label.set("world".into());
    assert_eq!(dom.to_html(node), r#"<ul><li class="item">world</li></ul>"#);

    css.set("active".into());
    assert_eq!(dom.to_html(node), r#"<ul><li class="active">world</li></ul>"#);
}
