//! End-to-end: a `.vrs` single-file component compiles into a render function
//! whose `<script>` Rust drives the `<template>` reactively, on `MockDom`.

use vue_rs_dom::MockDom;
use vue_rs_macro::component;

component!(counter, "tests/fixtures/counter.vrs");

#[test]
fn sfc_counter_renders_and_reacts() {
    let dom = MockDom::new();
    let node = counter(dom.clone(), Default::default());

    assert_eq!(dom.to_html(node), "<button>count is 0</button>");

    dom.dispatch(node, "click");
    assert_eq!(dom.to_html(node), "<button>count is 1</button>");

    dom.dispatch(node, "click");
    assert_eq!(dom.to_html(node), "<button>count is 2</button>");
}
