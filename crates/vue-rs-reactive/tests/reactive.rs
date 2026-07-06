//! Contract for the `Reactive` trait and the `reactive()` constructor,
//! independent of the derive macro (a hand-written impl stands in for what
//! `#[derive(Reactive)]` generates).

use vue_rs_reactive::{effect, reactive, signal, Reactive, Signal};
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
