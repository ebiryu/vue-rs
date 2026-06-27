//! Behavior contract for the reactive core.
//! These tests pin the PUBLIC API and observable semantics so the internal
//! backend can later be swapped (e.g. to an alien-signals style graph) freely.

use std::cell::RefCell;
use std::rc::Rc;

use vue_rs_reactive::{batch, computed, effect, signal};

#[test]
fn signal_get_returns_initial_value() {
    let count = signal(0);
    assert_eq!(count.get(), 0);
}

#[test]
fn signal_set_updates_value() {
    let count = signal(1);
    count.set(5);
    assert_eq!(count.get(), 5);
}

#[test]
fn signal_update_mutates_in_place() {
    let items = signal(vec![1, 2]);
    items.update(|v| v.push(3));
    assert_eq!(items.get(), vec![1, 2, 3]);
}

#[test]
fn signal_with_borrows_without_clone() {
    let name = signal(String::from("vue"));
    let len = name.with(|s| s.len());
    assert_eq!(len, 3);
}

#[test]
fn signals_are_copy_and_movable_into_closures() {
    // Handles must be Copy so they can be captured by multiple closures.
    let a = signal(10);
    let read_once = move || a.get();
    let read_twice = move || a.get();
    assert_eq!(read_once(), 10);
    assert_eq!(read_twice(), 10);
}

#[test]
fn effect_runs_immediately_once() {
    let runs = Rc::new(RefCell::new(0));
    let r = runs.clone();
    effect(move || {
        *r.borrow_mut() += 1;
    });
    assert_eq!(*runs.borrow(), 1);
}

#[test]
fn effect_reruns_when_dependency_changes() {
    let count = signal(0);
    let seen = Rc::new(RefCell::new(Vec::new()));
    let s = seen.clone();
    effect(move || {
        s.borrow_mut().push(count.get());
    });
    count.set(1);
    count.set(2);
    assert_eq!(*seen.borrow(), vec![0, 1, 2]);
}

#[test]
fn effect_does_not_rerun_for_untracked_signal() {
    let tracked = signal(0);
    let untracked = signal(100);
    let runs = Rc::new(RefCell::new(0));
    let r = runs.clone();
    effect(move || {
        let _ = tracked.get();
        *r.borrow_mut() += 1;
    });
    assert_eq!(*runs.borrow(), 1);
    untracked.set(200); // not read inside the effect -> no rerun
    assert_eq!(*runs.borrow(), 1);
    tracked.set(1);
    assert_eq!(*runs.borrow(), 2);
}

#[test]
fn computed_derives_from_signal() {
    let count = signal(2);
    let doubled = computed(move || count.get() * 2);
    assert_eq!(doubled.get(), 4);
    count.set(5);
    assert_eq!(doubled.get(), 10);
}

#[test]
fn computed_is_memoized_until_dependency_changes() {
    let count = signal(1);
    let computations = Rc::new(RefCell::new(0));
    let c = computations.clone();
    let derived = computed(move || {
        *c.borrow_mut() += 1;
        count.get() + 1
    });
    assert_eq!(derived.get(), 2);
    assert_eq!(derived.get(), 2); // cached: no recomputation
    assert_eq!(*computations.borrow(), 1);
    count.set(9);
    assert_eq!(derived.get(), 10);
    assert_eq!(*computations.borrow(), 2);
}

#[test]
fn effect_tracks_computed_dependency() {
    let count = signal(1);
    let doubled = computed(move || count.get() * 2);
    let seen = Rc::new(RefCell::new(Vec::new()));
    let s = seen.clone();
    effect(move || {
        s.borrow_mut().push(doubled.get());
    });
    count.set(3);
    assert_eq!(*seen.borrow(), vec![2, 6]);
}

#[test]
fn dynamic_dependencies_are_cleaned_up() {
    // When a branch stops being read, changes to it must not retrigger.
    let switch = signal(true);
    let a = signal(1);
    let b = signal(2);
    let runs = Rc::new(RefCell::new(0));
    let r = runs.clone();
    effect(move || {
        let _ = if switch.get() { a.get() } else { b.get() };
        *r.borrow_mut() += 1;
    });
    assert_eq!(*runs.borrow(), 1);

    switch.set(false); // now depends on b, not a
    assert_eq!(*runs.borrow(), 2);

    a.set(100); // a no longer tracked -> no rerun
    assert_eq!(*runs.borrow(), 2);

    b.set(200); // b is tracked -> rerun
    assert_eq!(*runs.borrow(), 3);
}

#[test]
fn batch_coalesces_writes_into_one_effect_run() {
    let a = signal(0);
    let b = signal(0);
    let runs = Rc::new(RefCell::new(0));
    let sum = Rc::new(RefCell::new(0));
    let (r, s) = (runs.clone(), sum.clone());
    effect(move || {
        *s.borrow_mut() = a.get() + b.get();
        *r.borrow_mut() += 1;
    });
    assert_eq!(*runs.borrow(), 1);

    batch(|| {
        a.set(10);
        b.set(20);
    });
    assert_eq!(*runs.borrow(), 2); // one rerun for two writes
    assert_eq!(*sum.borrow(), 30);

    // Without a batch, each write reruns the effect.
    a.set(1);
    b.set(2);
    assert_eq!(*runs.borrow(), 4);
}

#[test]
fn batch_returns_inner_value() {
    let value = batch(|| 7);
    assert_eq!(value, 7);
}

#[test]
#[allow(clippy::identity_op)] // formulas mirror the computed exprs for readability
fn diamond_runs_effect_once_per_change() {
    // a -> b, a -> c, effect reads b and c. One change to `a` => one effect run.
    let a = signal(1);
    let b = computed(move || a.get() + 1);
    let c = computed(move || a.get() * 2);
    let runs = Rc::new(RefCell::new(0));
    let r = runs.clone();
    let sum = Rc::new(RefCell::new(0));
    let s = sum.clone();
    effect(move || {
        *s.borrow_mut() = b.get() + c.get();
        *r.borrow_mut() += 1;
    });
    assert_eq!(*runs.borrow(), 1);
    assert_eq!(*sum.borrow(), (1 + 1) + (1 * 2)); // 4
    a.set(10);
    assert_eq!(*sum.borrow(), (10 + 1) + (10 * 2)); // 31
    assert_eq!(*runs.borrow(), 2); // exactly one rerun, not two
}
