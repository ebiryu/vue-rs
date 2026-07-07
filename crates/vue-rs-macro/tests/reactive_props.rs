//! End-to-end: a `#[derive(Reactive)]` struct flows down to a child as a
//! *read-only view* prop. The parent owns the mutable companion
//! (`reactive(..)`), the child declares the prop as the generated `XReadonly`
//! view, and the parent's mutations are observed through the view. The child
//! cannot mutate parent state (the view's fields are `ReadSignal`), enforcing
//! one-way data flow for composite reactive props.

use vue_rs_dom::MockDom;
use vue_rs_macro::{component, view, Reactive};
use vue_rs_reactive::reactive;

#[derive(Reactive)]
struct Counter {
    count: i32,
    label: String,
}

component!(Display, "tests/fixtures/display.vrs");

#[test]
fn reactive_struct_flows_down_as_a_readonly_view_prop() {
    let dom = MockDom::new();
    let state = reactive(Counter {
        count: 0,
        label: "hi".into(),
    });

    // The parent passes the mutable companion directly; codegen converts it to
    // the child's read-only view via `Into::into` (`From<CounterReactive> for
    // CounterReadonly`).
    let node = view!(dom.clone(), r#"<Display :state="state" />"#);
    assert_eq!(dom.to_html(node), "<p>0: hi</p>");

    // The prop is reactive: mutating the parent's companion updates the child
    // through the shared nodes of the read-only view.
    state.count.set(5);
    assert_eq!(dom.to_html(node), "<p>5: hi</p>");
    state.label.set("yo".into());
    assert_eq!(dom.to_html(node), "<p>5: yo</p>");
}
