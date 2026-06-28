//! End-to-end: a `.vrs` component can use `computed` (a deduplicating derived
//! value) in its `<script>` to drive the template.

use vue_rs_dom::MockDom;
use vue_rs_macro::component;

component!(Doubler, "tests/fixtures/doubler.vrs");

#[test]
fn computed_drives_template() {
    let dom = MockDom::new();
    let node = Doubler(dom.clone(), Default::default());
    assert_eq!(dom.to_html(node), "<button>0</button>");
    dom.dispatch(node, "click");
    assert_eq!(dom.to_html(node), "<button>2</button>");
    dom.dispatch(node, "click");
    assert_eq!(dom.to_html(node), "<button>4</button>");
}
