//! End-to-end: a component `v-model` lowers to prop-down / emit-up. The child
//! receives a read-only `model_value` (it cannot mutate parent state) and pushes
//! updates back through the `on_update_model_value` emit. Both directions flow.

use vue_rs_dom::MockDom;
use vue_rs_macro::{component, view};
use vue_rs_reactive::signal;

component!(Field, "tests/fixtures/field.vrs");

#[test]
fn component_v_model_flows_down_readonly_and_up_via_emit() {
    let dom = MockDom::new();
    let count = signal(0);

    // `<Field v-model="count" />` passes `count` down read-only and wires the
    // child's emit to write it.
    let node = view!(dom.clone(), r#"<Field v-model="count" />"#);
    assert_eq!(dom.to_html(node), "<button>0</button>");

    // The child emits a new value; it flows up into the parent's signal and back
    // down into the child's read-only view.
    dom.dispatch(node, "click");
    assert_eq!(count.get(), 1);
    assert_eq!(dom.to_html(node), "<button>1</button>");

    // Updating the parent's signal reflects down into the child.
    count.set(9);
    assert_eq!(dom.to_html(node), "<button>9</button>");
}
