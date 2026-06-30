//! End-to-end: the `ref="name"` directive binds an element's node into a
//! `template_ref()` handle, readable once the element is mounted.

// `El` must be in scope for `view!`'s generated builder code. `template_ref` is
// referenced fully-qualified below: the `component!` macro lifts the fixture's
// own `use vue_rs_dom::template_ref;` to module level, so importing it here too
// would collide.
use vue_rs_dom::{El, MockDom};
use vue_rs_macro::{component, view};
use vue_rs_reactive::flush_mounted;

#[test]
fn ref_directive_populates_the_handle_at_build_time() {
    let dom = MockDom::new();
    let field = vue_rs_dom::template_ref();

    let _node = view!(dom.clone(), r#"<input ref="field" />"#);

    // `node_ref` stores the node as the element is built, so the handle is
    // populated synchronously and points at the rendered `<input>`.
    let captured = field.get().expect("ref should hold the input node");
    assert_eq!(dom.to_html(captured), "<input></input>");
}

component!(InputRef, "tests/fixtures/input_ref.vrs");

#[test]
fn ref_declared_in_script_is_readable_in_on_mounted() {
    let dom = MockDom::new();
    let node = InputRef(dom.clone(), Default::default());

    // The ref is already populated at build time; `on_mounted` reads it after
    // the tree is built and flips the status.
    assert_eq!(
        dom.to_html(node),
        "<div><input></input><p>no-ref</p></div>",
        "before flush the on_mounted callback has not run"
    );

    flush_mounted();
    assert_eq!(
        dom.to_html(node),
        "<div><input></input><p>has-ref</p></div>",
        "on_mounted saw the populated template ref"
    );
}
