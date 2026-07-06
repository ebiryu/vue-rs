//! `#[derive(Reactive)]` turns a plain struct into a reactive companion whose
//! fields are each an independently tracked `Signal`.

use std::cell::RefCell;
use std::rc::Rc;

use vue_rs_macro::Reactive;
use vue_rs_reactive::{effect, reactive};

#[derive(Reactive)]
struct Counter {
    count: i32,
    label: String,
}

#[test]
fn reactive_exposes_each_field_as_a_signal() {
    let state = reactive(Counter {
        count: 0,
        label: "hi".into(),
    });
    assert_eq!(state.count.get(), 0);
    assert_eq!(state.label.get(), "hi");
    state.count.set(5);
    assert_eq!(state.count.get(), 5);
    state.label.set("bye".into());
    assert_eq!(state.label.get(), "bye");
}

#[test]
fn reactive_fields_drive_effects_independently() {
    let state = reactive(Counter {
        count: 0,
        label: "x".into(),
    });
    let seen = Rc::new(RefCell::new(Vec::new()));
    let s = seen.clone();
    effect(move || {
        s.borrow_mut().push(state.count.get());
    });
    // Writing an unread field must not rerun the effect.
    state.label.set("y".into());
    state.count.set(1);
    state.count.set(2);
    assert_eq!(*seen.borrow(), vec![0, 1, 2]);
}

#[test]
fn reactive_companion_is_copy() {
    let state = reactive(Counter {
        count: 7,
        label: "z".into(),
    });
    // Copy: both bindings still usable without a move error.
    let a = state;
    let b = state;
    assert_eq!(a.count.get(), 7);
    assert_eq!(b.count.get(), 7);
}

#[derive(Reactive)]
struct Point {
    x: i32,
    y: i32,
}

#[derive(Reactive)]
struct Shape {
    name: String,
    #[reactive]
    origin: Point,
}

#[test]
fn reactive_nests_reactive_fields() {
    let shape = reactive(Shape {
        name: "sq".into(),
        origin: Point { x: 1, y: 2 },
    });
    // A `#[reactive]` field is embedded as its companion, not `Signal<Point>`.
    assert_eq!(shape.origin.x.get(), 1);
    assert_eq!(shape.origin.y.get(), 2);
    assert_eq!(shape.name.get(), "sq");
    shape.origin.x.set(9);
    assert_eq!(shape.origin.x.get(), 9);
}

#[test]
fn reactive_nested_fields_drive_effects_independently() {
    let shape = reactive(Shape {
        name: "a".into(),
        origin: Point { x: 0, y: 0 },
    });
    let seen = Rc::new(RefCell::new(Vec::new()));
    let s = seen.clone();
    effect(move || {
        s.borrow_mut().push(shape.origin.x.get());
    });
    // Sibling nested field is independent.
    shape.origin.y.set(5);
    shape.origin.x.set(1);
    assert_eq!(*seen.borrow(), vec![0, 1]);
}

#[test]
fn reactive_nested_companion_is_copy() {
    let shape = reactive(Shape {
        name: "c".into(),
        origin: Point { x: 3, y: 4 },
    });
    let a = shape;
    let b = shape;
    assert_eq!(a.origin.x.get(), 3);
    assert_eq!(b.origin.y.get(), 4);
}
