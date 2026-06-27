//! End-to-end: `provide_context` in an ancestor reaches `use_context` in a
//! descendant component, scoped by the component ownership tree.

use vue_rs_dom::MockDom;
use vue_rs_macro::component;

component!(Provider, "tests/fixtures/provider.vrs");
component!(Themed, "tests/fixtures/themed.vrs");

#[test]
fn provided_value_reaches_descendant() {
    let dom = MockDom::new();
    let node = Provider(dom.clone());
    assert_eq!(dom.to_html(node), "<div><span>42</span></div>");
}

#[test]
fn missing_context_falls_back_to_default() {
    let dom = MockDom::new();
    // Rendered without a Provider ancestor: use_context returns None -> default 0.
    let node = Themed(dom.clone());
    assert_eq!(dom.to_html(node), "<span>0</span>");
}
