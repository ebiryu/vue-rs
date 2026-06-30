//! Contract for the fragment primitive (a template with multiple roots) against
//! `MockDom`: members mount, move, and unmount as a unit with no wrapper element.

use vue_rs_dom::{dyn_text_node, Backend, El, MockDom};
use vue_rs_reactive::{create_root, signal};

#[test]
fn fragment_serializes_members_without_a_wrapper() {
    let dom = MockDom::new();
    let h1 = El::new(dom.clone(), "h1").text("Title").finish();
    let p = El::new(dom.clone(), "p").text("Body").finish();
    let frag = dom.create_fragment(vec![h1, p]);

    assert_eq!(dom.to_html(frag), "<h1>Title</h1><p>Body</p>");
}

#[test]
fn fragment_appends_every_member_to_the_parent() {
    let dom = MockDom::new();
    let a = El::new(dom.clone(), "a").finish();
    let b = El::new(dom.clone(), "b").finish();
    let frag = dom.create_fragment(vec![a, b]);

    let root = El::new(dom.clone(), "div").child(frag).finish();

    assert_eq!(dom.to_html(root), "<div><a></a><b></b></div>");
}

#[test]
fn fragment_is_removed_as_a_unit() {
    let dom = MockDom::new();
    let a = El::new(dom.clone(), "a").finish();
    let b = El::new(dom.clone(), "b").finish();
    let frag = dom.create_fragment(vec![a, b]);
    let root = El::new(dom.clone(), "div").finish();
    dom.append_child(&root, &frag);
    assert_eq!(dom.to_html(root), "<div><a></a><b></b></div>");

    dom.remove_child(&root, &frag);
    assert_eq!(dom.to_html(root), "<div></div>");
}

#[test]
fn fragment_moves_as_a_unit_with_insert_before() {
    let dom = MockDom::new();
    let anchor = dom.create_anchor();
    let head = El::new(dom.clone(), "head").finish();
    let a = El::new(dom.clone(), "a").finish();
    let b = El::new(dom.clone(), "b").finish();
    let frag = dom.create_fragment(vec![a, b]);
    let root = El::new(dom.clone(), "div").finish();
    // Lay out: <head/> then the fragment, with a trailing anchor.
    dom.append_child(&root, &head);
    dom.append_child(&root, &anchor);
    dom.insert_before(&root, &frag, &anchor);
    assert_eq!(dom.to_html(root), "<div><head></head><a></a><b></b></div>");

    // Re-inserting the fragment before <head/> moves all its members together.
    dom.insert_before(&root, &frag, &head);
    assert_eq!(dom.to_html(root), "<div><a></a><b></b><head></head></div>");
}

#[test]
fn nested_fragments_flatten() {
    let dom = MockDom::new();
    let a = El::new(dom.clone(), "a").finish();
    let b = El::new(dom.clone(), "b").finish();
    let inner = dom.create_fragment(vec![a, b]);
    let c = El::new(dom.clone(), "c").finish();
    let outer = dom.create_fragment(vec![inner, c]);

    let root = El::new(dom.clone(), "div").child(outer).finish();
    assert_eq!(dom.to_html(root), "<div><a></a><b></b><c></c></div>");
}

#[test]
fn fragment_with_reactive_text_member_updates() {
    let dom = MockDom::new();
    let label = signal(String::from("hi"));
    let label_for_node = label;
    let (frag, _disposer) = {
        let dom = dom.clone();
        let mut built = None;
        let disposer = vue_rs_reactive::create_root_detached(|| {
            let text = dyn_text_node(&dom, move || label_for_node.get());
            let span = El::new(dom.clone(), "span").text("!").finish();
            built = Some(dom.create_fragment(vec![text, span]));
        });
        (built.unwrap(), disposer)
    };
    let root = El::new(dom.clone(), "div").child(frag).finish();

    assert_eq!(dom.to_html(root), "<div>hi<span>!</span></div>");
    label.set("bye".into());
    assert_eq!(dom.to_html(root), "<div>bye<span>!</span></div>");
}

#[test]
fn dyn_text_node_is_reactive() {
    create_root(|| {
        let dom = MockDom::new();
        let n = signal(0);
        let node = dyn_text_node(&dom, move || n.get().to_string());
        assert_eq!(dom.to_html(node), "0");
        n.set(7);
        assert_eq!(dom.to_html(node), "7");
    });
}
