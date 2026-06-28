//! Behavior contract for `watch(source, cb)`: a watcher that observes a specific
//! source getter and invokes a callback with the new and previous values.

use std::cell::RefCell;
use std::rc::Rc;

use vue_rs_reactive::{batch, create_root, signal, watch, watch_immediate};

#[test]
fn watch_does_not_fire_on_setup() {
    let count = signal(0);
    let calls = Rc::new(RefCell::new(Vec::<(i32, i32)>::new()));
    let c = calls.clone();
    watch(
        move || count.get(),
        move |new, old| c.borrow_mut().push((*new, *old)),
    );
    assert!(calls.borrow().is_empty());
}

#[test]
fn watch_fires_with_new_and_old_on_change() {
    let count = signal(0);
    let calls = Rc::new(RefCell::new(Vec::<(i32, i32)>::new()));
    let c = calls.clone();
    watch(
        move || count.get(),
        move |new, old| c.borrow_mut().push((*new, *old)),
    );

    count.set(1);
    count.set(5);
    assert_eq!(*calls.borrow(), vec![(1, 0), (5, 1)]);
}

#[test]
fn watch_does_not_fire_when_value_is_unchanged() {
    let count = signal(0);
    let calls = Rc::new(RefCell::new(0));
    let c = calls.clone();
    watch(move || count.get(), move |_, _| *c.borrow_mut() += 1);

    count.set(0); // dedup: no change
    assert_eq!(*calls.borrow(), 0);
    count.set(1);
    assert_eq!(*calls.borrow(), 1);
}

#[test]
fn watch_tracks_a_getter_over_multiple_sources() {
    let a = signal(1);
    let b = signal(2);
    let calls = Rc::new(RefCell::new(Vec::<i32>::new()));
    let c = calls.clone();
    watch(move || a.get() + b.get(), move |new, _| c.borrow_mut().push(*new));

    a.set(10); // 10 + 2
    b.set(20); // 10 + 20
    assert_eq!(*calls.borrow(), vec![12, 30]);
}

#[test]
fn watch_callback_reads_are_not_tracked() {
    let source = signal(0);
    let other = signal(100);
    let calls = Rc::new(RefCell::new(0));
    let c = calls.clone();
    watch(
        move || source.get(),
        move |_, _| {
            let _ = other.get(); // reading inside the callback must not subscribe
            *c.borrow_mut() += 1;
        },
    );

    source.set(1);
    assert_eq!(*calls.borrow(), 1);
    other.set(200); // must not re-run the watcher
    assert_eq!(*calls.borrow(), 1);
}

#[test]
fn watch_coalesces_batched_writes_into_one_call() {
    let count = signal(0);
    let calls = Rc::new(RefCell::new(Vec::<(i32, i32)>::new()));
    let c = calls.clone();
    watch(
        move || count.get(),
        move |new, old| c.borrow_mut().push((*new, *old)),
    );

    batch(|| {
        count.set(1);
        count.set(2);
        count.set(3);
    });
    // One firing with the original old value and the final new value.
    assert_eq!(*calls.borrow(), vec![(3, 0)]);
}

#[test]
fn watch_is_disposed_with_owning_scope() {
    let count = signal(0);
    let calls = Rc::new(RefCell::new(0));
    let c = calls.clone();
    let disposer = create_root(|| {
        watch(move || count.get(), move |_, _| *c.borrow_mut() += 1);
    });

    count.set(1);
    assert_eq!(*calls.borrow(), 1);
    disposer.dispose();
    count.set(2);
    assert_eq!(*calls.borrow(), 1, "disposed watcher must not fire");
}

#[test]
fn watch_immediate_fires_on_setup_with_no_old() {
    let count = signal(7);
    let calls = Rc::new(RefCell::new(Vec::<(i32, Option<i32>)>::new()));
    let c = calls.clone();
    watch_immediate(
        move || count.get(),
        move |new, old| c.borrow_mut().push((*new, old.copied())),
    );

    assert_eq!(*calls.borrow(), vec![(7, None)]);
    count.set(8);
    assert_eq!(*calls.borrow(), vec![(7, None), (8, Some(7))]);
}
