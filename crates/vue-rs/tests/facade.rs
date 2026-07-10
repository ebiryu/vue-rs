//! The facade re-exports the reactive core, the (native) DOM layer, and the
//! macros under one `vue_rs::` path.

use vue_rs::{signal, view, El, MockDom};

#[test]
fn facade_exposes_reactive_dom_and_view_macro() {
    let dom = MockDom::new();
    let count = signal(0);
    let node = view!(dom.clone(), r#"<p>{{ count.get() }}</p>"#);
    assert_eq!(dom.to_html(node), "<p>0</p>");
    count.set(5);
    assert_eq!(dom.to_html(node), "<p>5</p>");
}
