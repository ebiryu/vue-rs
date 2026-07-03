//! End-to-end: a named component `v-model:arg` binds the child's `arg` prop
//! (value down) and `on_update_<arg>` emit (up). (Separate test module:
//! `component!` lifts `use` items to module level, so each fixture's `use` lives
//! in its own file to avoid import clashes.)

use vue_rs_dom::MockDom;
use vue_rs_macro::{component, view};
use vue_rs_reactive::signal;

component!(NamedField, "tests/fixtures/named_field.vrs");

#[test]
fn named_component_v_model_flows_both_ways() {
    let dom = MockDom::new();
    let heading = signal(0);

    // `v-model:title` targets the child's `title` prop and `on_update_title` emit.
    let node = view!(dom.clone(), r#"<NamedField v-model:title="heading" />"#);
    assert_eq!(dom.to_html(node), "<button>0</button>");

    dom.dispatch(node, "click");
    assert_eq!(heading.get(), 1);
    assert_eq!(dom.to_html(node), "<button>1</button>");

    heading.set(9);
    assert_eq!(dom.to_html(node), "<button>9</button>");
}
