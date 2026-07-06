//! End-to-end: a child declares a `MaybeSignal<T>` prop, so the same component
//! accepts both a static value and a reactive source, and reactive sources keep
//! the child up to date.

use vue_rs_dom::MockDom;
use vue_rs_macro::{component, view};
use vue_rs_reactive::signal;

component!(Labeled, "tests/fixtures/labeled.vrs");

#[test]
fn maybe_signal_prop_accepts_a_static_value() {
    let dom = MockDom::new();
    let node = view!(dom.clone(), r#"<Labeled :label="String::from(\"hi\")" />"#);
    assert_eq!(dom.to_html(node), "<span>hi</span>");
}

#[test]
fn maybe_signal_prop_accepts_a_reactive_source_and_tracks_it() {
    let dom = MockDom::new();
    let label = signal(String::from("hi"));
    let node = view!(dom.clone(), r#"<Labeled :label="label" />"#);
    assert_eq!(dom.to_html(node), "<span>hi</span>");

    label.set(String::from("yo"));
    assert_eq!(dom.to_html(node), "<span>yo</span>");
}
