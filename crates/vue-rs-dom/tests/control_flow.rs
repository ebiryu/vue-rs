//! Contract for control-flow and value-event primitives against `MockDom`.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use vue_rs_dom::{El, MockDom};
use vue_rs_reactive::{create_root, effect, signal};

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
fn dyn_switch_selects_active_branch() {
    let dom = MockDom::new();
    let n = signal(0);
    let views: Vec<Box<dyn Fn(MockDom) -> usize>> = vec![
        Box::new(|b| El::new(b, "p").text("zero").finish()),
        Box::new(|b| El::new(b, "p").text("one").finish()),
        Box::new(|b| El::new(b, "p").text("many").finish()),
    ];
    let root = El::new(dom.clone(), "div")
        .dyn_switch(
            move || match n.get() {
                0 => Some(0),
                1 => Some(1),
                _ => Some(2),
            },
            views,
        )
        .finish();

    assert_eq!(dom.to_html(root), "<div><p>zero</p></div>");
    n.set(1);
    assert_eq!(dom.to_html(root), "<div><p>one</p></div>");
    n.set(5);
    assert_eq!(dom.to_html(root), "<div><p>many</p></div>");
    n.set(0);
    assert_eq!(dom.to_html(root), "<div><p>zero</p></div>");
}

#[test]
fn dyn_element_rebuilds_when_tag_changes() {
    let dom = MockDom::new();
    let tag = signal("h1".to_string());
    let root = El::new(dom.clone(), "div")
        .dyn_element(
            move || tag.get(),
            move |b, t| El::new(b, t).text("hi").finish(),
        )
        .finish();

    assert_eq!(dom.to_html(root), "<div><h1>hi</h1></div>");
    tag.set("h2".to_string());
    assert_eq!(dom.to_html(root), "<div><h2>hi</h2></div>");
    // The same tag is a no-op (the subtree is not rebuilt).
    tag.set("h2".to_string());
    assert_eq!(dom.to_html(root), "<div><h2>hi</h2></div>");
}

#[test]
fn dyn_element_branch_is_disposed_with_owning_scope() {
    let dom = MockDom::new();
    let tick = signal(0);
    let runs = Rc::new(Cell::new(0));
    let r = runs.clone();
    let disposer = create_root(|| {
        El::new(dom.clone(), "div")
            .dyn_element(
                || "p".to_string(),
                move |b, t| {
                    let r = r.clone();
                    El::new(b, t)
                        .dyn_text(move || {
                            r.set(r.get() + 1);
                            tick.get().to_string()
                        })
                        .finish()
                },
            )
            .finish();
    });

    assert_eq!(runs.get(), 1);
    tick.set(1);
    assert_eq!(runs.get(), 2);

    disposer.dispose();
    tick.set(2);
    assert_eq!(
        runs.get(),
        2,
        "the mounted element's effects must be disposed with the owning scope"
    );
}

#[test]
fn dyn_switch_mounts_nothing_when_no_branch_matches() {
    let dom = MockDom::new();
    let show = signal(false);
    let views: Vec<Box<dyn Fn(MockDom) -> usize>> =
        vec![Box::new(|b| El::new(b, "p").text("hi").finish())];
    let root = El::new(dom.clone(), "div")
        .dyn_switch(move || if show.get() { Some(0) } else { None }, views)
        .finish();

    assert_eq!(dom.to_html(root), "<div></div>");
    show.set(true);
    assert_eq!(dom.to_html(root), "<div><p>hi</p></div>");
    show.set(false);
    assert_eq!(dom.to_html(root), "<div></div>");
}

#[test]
fn dyn_switch_branch_is_disposed_with_owning_scope() {
    let dom = MockDom::new();
    let tick = signal(0);
    let runs = Rc::new(Cell::new(0));
    let r = runs.clone();
    let disposer = create_root(|| {
        let views: Vec<Box<dyn Fn(MockDom) -> usize>> = vec![Box::new(move |b| {
            let r = r.clone();
            El::new(b, "p")
                .dyn_text(move || {
                    r.set(r.get() + 1);
                    tick.get().to_string()
                })
                .finish()
        })];
        El::new(dom.clone(), "div")
            .dyn_switch(|| Some(0), views)
            .finish();
    });

    assert_eq!(runs.get(), 1);
    tick.set(1);
    assert_eq!(runs.get(), 2);

    disposer.dispose();
    tick.set(2);
    assert_eq!(
        runs.get(),
        2,
        "active branch effect must be disposed with the owning scope"
    );
}

#[test]
fn dyn_if_branch_is_disposed_with_owning_scope() {
    let dom = MockDom::new();
    let tick = signal(0);
    let runs = Rc::new(Cell::new(0));
    let r = runs.clone();
    let disposer = create_root(|| {
        El::new(dom.clone(), "div")
            .dyn_if(
                || true,
                move |b| {
                    let r = r.clone();
                    El::new(b, "p")
                        .dyn_text(move || {
                            r.set(r.get() + 1);
                            tick.get().to_string()
                        })
                        .finish()
                },
            )
            .finish();
    });

    assert_eq!(runs.get(), 1, "branch effect runs once on mount");
    tick.set(1);
    assert_eq!(runs.get(), 2, "mounted branch effect reacts");

    disposer.dispose();
    tick.set(2);
    assert_eq!(
        runs.get(),
        2,
        "branch effect must be disposed with the owning scope"
    );
}

#[test]
fn dyn_if_else_branch_is_disposed_with_owning_scope() {
    let dom = MockDom::new();
    let tick = signal(0);
    let runs = Rc::new(Cell::new(0));
    let r = runs.clone();
    let disposer = create_root(|| {
        El::new(dom.clone(), "div")
            .dyn_if_else(
                || true,
                move |b| {
                    let r = r.clone();
                    El::new(b, "p")
                        .dyn_text(move || {
                            r.set(r.get() + 1);
                            tick.get().to_string()
                        })
                        .finish()
                },
                move |b| El::new(b, "span").text("no").finish(),
            )
            .finish();
    });

    assert_eq!(runs.get(), 1);
    tick.set(1);
    assert_eq!(runs.get(), 2);

    disposer.dispose();
    tick.set(2);
    assert_eq!(
        runs.get(),
        2,
        "active branch effect must be disposed with the owning scope"
    );
}

#[test]
fn dyn_for_rows_are_disposed_with_owning_scope() {
    let dom = MockDom::new();
    let tick = signal(0);
    let runs = Rc::new(Cell::new(0));
    let r = runs.clone();
    let disposer = create_root(|| {
        El::new(dom.clone(), "ul")
            .dyn_for(
                move || vec![1],
                |n| *n,
                move |b, _n| {
                    let r = r.clone();
                    El::new(b, "li")
                        .dyn_text(move || {
                            r.set(r.get() + 1);
                            tick.get().to_string()
                        })
                        .finish()
                },
            )
            .finish();
    });

    assert_eq!(runs.get(), 1);
    tick.set(1);
    assert_eq!(runs.get(), 2);

    disposer.dispose();
    tick.set(2);
    assert_eq!(
        runs.get(),
        2,
        "row effects must be disposed with the owning scope"
    );
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
fn event_handler_batches_writes() {
    let dom = MockDom::new();
    let a = signal(0);
    let b = signal(0);
    let runs = Rc::new(RefCell::new(0));
    let r = runs.clone();

    let node = El::new(dom.clone(), "button")
        .on("click", move || {
            a.set(1);
            b.set(2);
        })
        .finish();
    effect(move || {
        a.get();
        b.get();
        *r.borrow_mut() += 1;
    });
    assert_eq!(*runs.borrow(), 1);

    dom.dispatch(node, "click"); // two writes, one batched effect run
    assert_eq!(*runs.borrow(), 2);
}

#[test]
fn dyn_for_adds_removes_and_reorders_keyed_rows() {
    let dom = MockDom::new();
    let items = signal(vec![1, 2, 3]);
    let root = El::new(dom.clone(), "ul")
        .dyn_for(
            move || items.get(),
            |n| *n,
            move |b, n| El::new(b, "li").dyn_text(move || n.get().to_string()).finish(),
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

#[derive(Clone, PartialEq)]
struct Row {
    id: u32,
    name: &'static str,
}

#[test]
fn dyn_for_patches_row_when_item_data_changes_for_a_stable_key() {
    let dom = MockDom::new();
    let items = signal(vec![
        Row { id: 1, name: "a" },
        Row { id: 2, name: "b" },
    ]);
    let root = El::new(dom.clone(), "ul")
        .dyn_for(
            move || items.get(),
            |r| r.id,
            move |b, r| El::new(b, "li").dyn_text(move || r.get().name.to_string()).finish(),
        )
        .finish();

    assert_eq!(dom.to_html(root), "<ul><li>a</li><li>b</li></ul>");

    // Same keys (ids), but row 1's data changed: the row's binding must update.
    items.set(vec![
        Row { id: 1, name: "A" },
        Row { id: 2, name: "b" },
    ]);
    assert_eq!(dom.to_html(root), "<ul><li>A</li><li>b</li></ul>");
}

#[test]
fn dyn_for_does_not_rebuild_rows_on_data_change() {
    let dom = MockDom::new();
    let items = signal(vec![
        Row { id: 1, name: "a" },
        Row { id: 2, name: "b" },
    ]);
    let builds = Rc::new(Cell::new(0));
    let b = builds.clone();
    let _root = El::new(dom.clone(), "ul")
        .dyn_for(
            move || items.get(),
            |r| r.id,
            move |backend, r| {
                b.set(b.get() + 1);
                El::new(backend, "li").dyn_text(move || r.get().name.to_string()).finish()
            },
        )
        .finish();

    assert_eq!(builds.get(), 2);

    // A data change under stable keys must patch, not rebuild: no new row builds.
    items.set(vec![
        Row { id: 1, name: "A" },
        Row { id: 2, name: "b" },
    ]);
    assert_eq!(builds.get(), 2, "rows are patched via their signal, not rebuilt");
}

#[test]
fn dyn_for_indexed_index_signal_updates_on_reorder() {
    let dom = MockDom::new();
    let items = signal(vec![10, 20]);
    let root = El::new(dom.clone(), "ul")
        .dyn_for_indexed(
            move || items.get(),
            |n| *n,
            move |b, n, i| {
                El::new(b, "li")
                    .dyn_text(move || format!("{}:{}", i.get(), n.get()))
                    .finish()
            },
        )
        .finish();

    assert_eq!(dom.to_html(root), "<ul><li>0:10</li><li>1:20</li></ul>");

    // Reordering keeps each row (keyed by value) but its index changes reactively.
    items.set(vec![20, 10]);
    assert_eq!(dom.to_html(root), "<ul><li>0:20</li><li>1:10</li></ul>");
}
