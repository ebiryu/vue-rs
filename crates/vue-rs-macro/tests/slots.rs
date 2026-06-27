//! End-to-end: a component with a `<slot>` renders parent-provided children,
//! and the slotted content stays reactive.

use vue_rs_dom::{El, MockDom};
use vue_rs_macro::{component, view};
use vue_rs_reactive::signal;

component!(Card, "tests/fixtures/card.vrs");
component!(Layout, "tests/fixtures/layout.vrs");

#[test]
fn slot_renders_parent_children() {
    let dom = MockDom::new();
    let node = view!(dom.clone(), r#"<Card><span>hello</span></Card>"#);
    assert_eq!(
        dom.to_html(node),
        r#"<div class="card"><span>hello</span></div>"#
    );
}

#[test]
fn slotted_content_is_reactive() {
    let dom = MockDom::new();
    let label = signal(String::from("hi"));
    let node = view!(
        dom.clone(),
        r#"<Card><p>{{ label.get() }}</p></Card>"#
    );
    assert_eq!(dom.to_html(node), r#"<div class="card"><p>hi</p></div>"#);
    label.set("bye".into());
    assert_eq!(dom.to_html(node), r#"<div class="card"><p>bye</p></div>"#);
}

#[test]
fn named_and_default_slots() {
    let dom = MockDom::new();
    let node = view!(
        dom.clone(),
        r#"<Layout><template v-slot:header><h1>Title</h1></template><p>Body</p></Layout>"#
    );
    assert_eq!(
        dom.to_html(node),
        "<div><header><h1>Title</h1></header><main><p>Body</p></main></div>"
    );
}

#[test]
fn absent_named_slot_renders_nothing() {
    let dom = MockDom::new();
    // Only the default slot is provided; the `header` slot falls back to empty.
    let node = view!(dom.clone(), r#"<Layout><p>Body</p></Layout>"#);
    assert_eq!(
        dom.to_html(node),
        "<div><header></header><main><p>Body</p></main></div>"
    );
}
