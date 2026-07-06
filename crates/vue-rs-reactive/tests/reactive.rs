//! Contract for the `Reactive` trait and the `reactive()` constructor,
//! independent of the derive macro (a hand-written impl stands in for what
//! `#[derive(Reactive)]` generates).

use vue_rs_reactive::{effect, reactive, readonly, signal, Reactive, ReadSignal, Readonly, Signal};
use std::cell::RefCell;
use std::rc::Rc;

struct Point {
    x: i32,
    y: i32,
}

#[derive(Clone, Copy)]
struct PointReactive {
    x: Signal<i32>,
    y: Signal<i32>,
}

impl Reactive for Point {
    type Target = PointReactive;
    fn into_reactive(self) -> Self::Target {
        PointReactive {
            x: signal(self.x),
            y: signal(self.y),
        }
    }
}

#[test]
fn reactive_moves_each_field_into_a_signal() {
    let p = reactive(Point { x: 1, y: 2 });
    assert_eq!(p.x.get(), 1);
    assert_eq!(p.y.get(), 2);
    p.x.set(10);
    assert_eq!(p.x.get(), 10);
    assert_eq!(p.y.get(), 2);
}

#[test]
fn reactive_fields_track_independently() {
    let p = reactive(Point { x: 0, y: 0 });
    let seen = Rc::new(RefCell::new(Vec::new()));
    let s = seen.clone();
    effect(move || {
        s.borrow_mut().push(p.x.get());
    });
    p.y.set(5); // unread field: no rerun
    p.x.set(1);
    assert_eq!(*seen.borrow(), vec![0, 1]);
}

// The read-only view mirrors the companion with each `Signal` projected to a
// `ReadSignal` (same node). This stands in for what `#[derive(Reactive)]`
// generates for `readonly()`.
#[derive(Clone, Copy)]
struct PointReadonly {
    x: ReadSignal<i32>,
    y: ReadSignal<i32>,
}

impl Readonly for PointReactive {
    type Target = PointReadonly;
    fn into_readonly(self) -> Self::Target {
        PointReadonly {
            x: readonly(self.x),
            y: readonly(self.y),
        }
    }
}

#[test]
fn readonly_of_a_signal_is_a_read_only_view_of_the_same_node() {
    let s = signal(5);
    let r: ReadSignal<i32> = readonly(s);
    assert_eq!(r.get(), 5);
    // Writing the source is visible through the read-only view.
    s.set(6);
    assert_eq!(r.get(), 6);
}

#[test]
fn readonly_view_reads_the_same_values_as_the_source() {
    let p = reactive(Point { x: 1, y: 2 });
    let ro = readonly(p);
    assert_eq!(ro.x.get(), 1);
    assert_eq!(ro.y.get(), 2);
    // Writes through the source are observed by the read-only view.
    p.x.set(9);
    assert_eq!(ro.x.get(), 9);
}

#[test]
fn readonly_view_tracks_dependencies() {
    let p = reactive(Point { x: 0, y: 0 });
    let ro = readonly(p);
    let seen = Rc::new(RefCell::new(Vec::new()));
    let s = seen.clone();
    effect(move || {
        s.borrow_mut().push(ro.x.get());
    });
    p.y.set(5); // unread field: no rerun
    p.x.set(1);
    assert_eq!(*seen.borrow(), vec![0, 1]);
}
