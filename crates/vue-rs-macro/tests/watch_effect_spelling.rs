//! End-to-end: a `.vrs` `<script>` written with Vue's `watchEffect(...)`
//! spelling (and its `use ...::watchEffect` import) compiles — the SFC compiler
//! maps `watchEffect` onto the core's `effect` — and the effect re-runs
//! reactively, here mirroring `count * 2` into a second signal.

use vue_rs_dom::MockDom;
use vue_rs_macro::component;

component!(counter_watch_effect, "tests/fixtures/counter_watch_effect.vrs");

#[test]
fn sfc_watch_effect_spelling_reacts() {
    let dom = MockDom::new();
    let node = counter_watch_effect(dom.clone(), Default::default());

    assert_eq!(dom.to_html(node), "<button>0/0</button>");

    dom.dispatch(node, "click");
    assert_eq!(dom.to_html(node), "<button>1/2</button>");

    dom.dispatch(node, "click");
    assert_eq!(dom.to_html(node), "<button>2/4</button>");
}
