//! End-to-end: a child declares optional props via `#[prop(default)]` /
//! `#[prop(default = expr)]`. The parent may omit them (the default is used) or
//! provide them (the value flows in), while required props stay mandatory.

use vue_rs_dom::MockDom;
use vue_rs_macro::{component, view};
use vue_rs_reactive::signal;

component!(Stepper, "tests/fixtures/stepper.vrs");

#[test]
fn omitted_default_prop_uses_the_default() {
    let dom = MockDom::new();
    let total = signal(0);
    let value = signal(10);

    // `:step` and `@update` are both omitted: step defaults to 1, and the
    // unlistened emit is dropped (no panic).
    let node = view!(dom.clone(), r#"<Stepper :value="value" />"#);
    assert_eq!(dom.to_html(node), "<button>10</button>");

    dom.dispatch(node, "click");
    assert_eq!(total.get(), 0); // no listener attached, emit is a no-op
}

#[test]
fn provided_default_prop_overrides_the_default() {
    let dom = MockDom::new();
    let total = signal(0);
    let value = signal(10);

    // `:step` overrides the default and `@update` is listened.
    let node = view!(
        dom.clone(),
        r#"<Stepper :value="value" :step="5" @update="move |n: i32| total.set(n)" />"#
    );
    assert_eq!(dom.to_html(node), "<button>10</button>");

    dom.dispatch(node, "click");
    assert_eq!(total.get(), 15); // 10 + step(5)
}
