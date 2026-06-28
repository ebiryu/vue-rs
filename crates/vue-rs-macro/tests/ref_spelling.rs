//! End-to-end: a `.vrs` `<script>` written with Vue's `ref(...)` spelling (and
//! its `use ...::ref` import) compiles — the SFC compiler maps the keyword
//! `ref` onto the keyword-safe core constructor `signal` — and stays reactive.

use vue_rs_dom::MockDom;
use vue_rs_macro::component;

component!(counter_ref, "tests/fixtures/counter_ref.vrs");

#[test]
fn sfc_counter_with_vue_ref_spelling_reacts() {
    let dom = MockDom::new();
    let node = counter_ref(dom.clone(), Default::default());

    assert_eq!(dom.to_html(node), "<button>count is 0</button>");

    dom.dispatch(node, "click");
    assert_eq!(dom.to_html(node), "<button>count is 1</button>");

    dom.dispatch(node, "click");
    assert_eq!(dom.to_html(node), "<button>count is 2</button>");
}
