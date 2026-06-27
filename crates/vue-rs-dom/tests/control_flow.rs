//! Contract for control-flow and value-event primitives against `MockDom`.

use std::cell::RefCell;
use std::rc::Rc;

use vue_rs_dom::{El, MockDom};
use vue_rs_reactive::signal;

#[test]
fn dyn_if_mounts_and_unmounts() {
    let dom = MockDom::new();
    let show = signal(false);
    let root = El::new(dom.clone(), "div")
        .dyn_if(
            move || show.get(),
            move |b| El::new(b, "p").text("hi").finish(),
        )
        .finish();

    assert_eq!(dom.to_html(root), "<div></div>");
    show.set(true);
    assert_eq!(dom.to_html(root), "<div><p>hi</p></div>");
    show.set(false);
    assert_eq!(dom.to_html(root), "<div></div>");
}

#[test]
fn dyn_if_else_swaps_branches() {
    let dom = MockDom::new();
    let toggle = signal(true);
    let root = El::new(dom.clone(), "div")
        .dyn_if_else(
            move || toggle.get(),
            move |b| El::new(b, "p").text("yes").finish(),
            move |b| El::new(b, "span").text("no").finish(),
        )
        .finish();

    assert_eq!(dom.to_html(root), "<div><p>yes</p></div>");
    toggle.set(false);
    assert_eq!(dom.to_html(root), "<div><span>no</span></div>");
    toggle.set(true);
    assert_eq!(dom.to_html(root), "<div><p>yes</p></div>");
}

#[test]
fn on_value_receives_dispatched_value() {
    let dom = MockDom::new();
    let captured: Rc<RefCell<String>> = Rc::new(RefCell::new(String::new()));
    let sink = captured.clone();
    let node = El::new(dom.clone(), "input")
        .on_value("input", move |v| *sink.borrow_mut() = v.to_string())
        .finish();

    dom.dispatch_value(node, "input", "hello");
    assert_eq!(*captured.borrow(), "hello");
}

#[test]
fn dyn_for_adds_removes_and_reorders_keyed_rows() {
    let dom = MockDom::new();
    let items = signal(vec![1, 2, 3]);
    let root = El::new(dom.clone(), "ul")
        .dyn_for(
            move || items.get(),
            |n| *n,
            move |b, n| El::new(b, "li").text(&n.to_string()).finish(),
        )
        .finish();

    assert_eq!(dom.to_html(root), "<ul><li>1</li><li>2</li><li>3</li></ul>");

    items.set(vec![1, 3]); // remove 2
    assert_eq!(dom.to_html(root), "<ul><li>1</li><li>3</li></ul>");

    items.set(vec![3, 1]); // reorder
    assert_eq!(dom.to_html(root), "<ul><li>3</li><li>1</li></ul>");

    items.set(vec![3, 1, 4]); // add 4
    assert_eq!(dom.to_html(root), "<ul><li>3</li><li>1</li><li>4</li></ul>");
}
