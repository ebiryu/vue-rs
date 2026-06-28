//! End-to-end: `on_mounted` callbacks run when `flush_mounted` is called after
//! the tree is built, and their effects propagate reactively.

use vue_rs_dom::MockDom;
use vue_rs_macro::component;
use vue_rs_reactive::flush_mounted;

component!(Widget, "tests/fixtures/widget.vrs");

#[test]
fn on_mounted_runs_after_flush() {
    let dom = MockDom::new();
    let node = Widget(dom.clone(), Default::default());

    // Before mount flush, the on_mounted callback has not run.
    assert_eq!(dom.to_html(node), "<p>pending</p>");

    flush_mounted();
    assert_eq!(dom.to_html(node), "<p>mounted</p>");
}
