//! End-to-end: a `.vrs` `<script>` using `watch(source, cb)` compiles (the core
//! function is named `watch`, so no remap is needed — like `computed`) and the
//! callback runs on change with the new value, here mirroring `count * 2` into a
//! second signal. Unlike `watchEffect`, it does not run on setup.

use vue_rs_dom::MockDom;
use vue_rs_macro::component;

component!(counter_watch, "tests/fixtures/counter_watch.vrs");

#[test]
fn sfc_watch_reacts_on_change() {
    let dom = MockDom::new();
    let node = counter_watch(dom.clone(), Default::default());

    // watch does not fire on setup, so `doubled` stays at its initial 0.
    assert_eq!(dom.to_html(node), "<button>0/0</button>");

    dom.dispatch(node, "click");
    assert_eq!(dom.to_html(node), "<button>1/2</button>");

    dom.dispatch(node, "click");
    assert_eq!(dom.to_html(node), "<button>2/4</button>");
}
