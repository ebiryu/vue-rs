//! `SlotFn<B, T>` is the typed slot channel: a parent-supplied builder the child
//! invokes with a typed payload. Components hold each slot as an
//! `Option<SlotFn<B, T>>` field on a generated `NameSlots<B>` struct, so an
//! absent slot is a graceful `None` the child renders as fallback.

use vue_rs_dom::{El, MockDom, SlotFn};

#[test]
fn slot_fn_renders_with_typed_payload() {
    struct Row {
        index: i32,
    }
    let dom = MockDom::new();
    let slot = SlotFn::new(|backend: MockDom, row: Row| {
        El::new(backend, "li")
            .dyn_text(move || row.index.to_string())
            .finish()
    });
    let node = slot.render(dom.clone(), Row { index: 3 });
    assert_eq!(dom.to_html(node), "<li>3</li>");
}
