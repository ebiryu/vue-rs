//! End-to-end: a child component declares props (reactive `:label`) and an emit
//! (`@total`); the parent passes data down and receives emitted values up.

use vue_rs_dom::MockDom;
use vue_rs_macro::{component, view};
use vue_rs_reactive::signal;

component!(Greeter, "tests/fixtures/greeter.vrs");

#[test]
fn props_flow_down_and_emits_flow_up() {
    let dom = MockDom::new();
    let total = signal(0);
    let label = signal(String::from("hi"));

    let node = view!(
        dom.clone(),
        r#"<Greeter :label="label" @total="move |n: i32| total.set(n)" />"#
    );

    assert_eq!(dom.to_html(node), "<button>hi: 0</button>");

    dom.dispatch(node, "click");
    assert_eq!(dom.to_html(node), "<button>hi: 1</button>");
    assert_eq!(total.get(), 1); // emit flowed up to the parent

    dom.dispatch(node, "click");
    assert_eq!(total.get(), 2);

    // The prop is reactive: updating the parent's signal updates the child.
    label.set("yo".into());
    assert_eq!(dom.to_html(node), "<button>yo: 2</button>");
}
