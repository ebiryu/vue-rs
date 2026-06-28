//! End-to-end: a component with a `<slot>` renders parent-provided children,
//! and the slotted content stays reactive.

use vue_rs_dom::{El, MockDom};
use vue_rs_macro::{component, view};
use vue_rs_reactive::signal;

component!(Card, "tests/fixtures/card.vrs");
component!(Layout, "tests/fixtures/layout.vrs");
component!(Badge, "tests/fixtures/badge.vrs");
component!(Feed, "tests/fixtures/feed.vrs");
component!(Panel, "tests/fixtures/panel.vrs");

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
    let node = view!(dom.clone(), r#"<Card><p>{{ label.get() }}</p></Card>"#);
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
fn scoped_slot_passes_data_to_default_slot() {
    let dom = MockDom::new();
    // The child's `<slot :label="text">` hands a `BadgeData { label }` payload
    // to the parent, which binds it as `d` (type inferred) and reads `d.label`.
    let node = view!(
        dom.clone(),
        r#"<Badge><template v-slot:default="d"><span>{{ d.label }}</span></template></Badge>"#
    );
    assert_eq!(
        dom.to_html(node),
        r#"<div class="badge"><span>new</span></div>"#
    );
}

#[test]
fn scoped_named_slot_passes_data() {
    let dom = MockDom::new();
    // A named slot can also be scoped: `<slot name="row" :index="3">` builds a
    // `RowData { index }` payload the parent reads as `d.index`.
    let node = view!(
        dom.clone(),
        r#"<Feed><template v-slot:row="d"><li>{{ d.index }}</li></template></Feed>"#
    );
    assert_eq!(dom.to_html(node), "<ul><li>3</li></ul>");
}

#[test]
fn partial_scoped_slots_fall_back_to_default_content() {
    let dom = MockDom::new();
    // Panel declares scoped `head` and `foot`; the parent provides only `head`,
    // so `foot` renders the fallback content written inside its `<slot>`.
    let node = view!(
        dom.clone(),
        r#"<Panel><template v-slot:head="d"><h2>{{ d.title }}</h2></template></Panel>"#
    );
    assert_eq!(
        dom.to_html(node),
        r#"<div class="panel"><h2>Hi</h2><em>none</em></div>"#
    );
}

#[test]
fn optional_scoped_slot_uses_fallback_when_omitted() {
    let dom = MockDom::new();
    // The other way around: provide only `foot`, so `head` falls back.
    let node = view!(
        dom.clone(),
        r#"<Panel><template v-slot:foot="d"><small>{{ d.year }}</small></template></Panel>"#
    );
    assert_eq!(
        dom.to_html(node),
        r#"<div class="panel"><span>?</span><small>2026</small></div>"#
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

#[test]
fn component_with_scoped_slots_needs_no_slots_provided() {
    let dom = MockDom::new();
    // The parent provides no slots at all; every scoped slot falls back.
    let node = view!(dom.clone(), r#"<Panel></Panel>"#);
    assert_eq!(
        dom.to_html(node),
        r#"<div class="panel"><span>?</span><em>none</em></div>"#
    );
}

#[test]
fn component_with_plain_slot_needs_no_content_provided() {
    let dom = MockDom::new();
    // A plain `<slot>` with nothing provided falls back to an empty anchor.
    let node = view!(dom.clone(), r#"<Card></Card>"#);
    assert_eq!(dom.to_html(node), r#"<div class="card"></div>"#);
}
