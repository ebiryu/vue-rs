//! Contract for `MaybeSignal<T>`: a prop value that is either a static value or
//! a reactive source (signal / memo / read-only view / derived closure). Reading
//! it tracks dependencies when it wraps a reactive source and is inert when it
//! wraps a static value, so a child component can accept both spellings behind
//! one prop type.

use std::cell::RefCell;
use std::rc::Rc;

use vue_rs_reactive::{computed, effect, signal, MaybeSignal, Signal};

#[test]
fn static_value_reads_back() {
    let v: MaybeSignal<i32> = MaybeSignal::Static(5);
    assert_eq!(v.get(), 5);
    assert_eq!(v.with(|n| *n + 1), 6);
}

#[test]
fn plain_value_via_into_is_static() {
    // The component macro wraps every prop expression in `.into()`, so a plain
    // literal must land in the `Static` arm.
    let v: MaybeSignal<i32> = 7.into();
    assert_eq!(v.get(), 7);
}

#[test]
fn from_a_signal_reads_and_tracks_updates() {
    let count = signal(0);
    let v: MaybeSignal<i32> = count.into();
    assert_eq!(v.get(), 0);

    let seen = Rc::new(RefCell::new(Vec::new()));
    let seen2 = seen.clone();
    let probe = v.clone();
    effect(move || seen2.borrow_mut().push(probe.get()));

    count.set(1);
    count.set(2);
    assert_eq!(*seen.borrow(), vec![0, 1, 2]);
}

#[test]
fn from_a_memo_reads_and_tracks_updates() {
    let count = signal(2);
    let doubled = computed(move || count.get() * 2);
    let v: MaybeSignal<i32> = doubled.into();
    assert_eq!(v.get(), 4);
    count.set(5);
    assert_eq!(v.get(), 10);
}

#[test]
fn derived_closure_reads_and_tracks() {
    let a = signal(1);
    let b = signal(10);
    let v = MaybeSignal::derive(move || a.get() + b.get());
    assert_eq!(v.get(), 11);

    let seen = Rc::new(RefCell::new(Vec::new()));
    let seen2 = seen.clone();
    let probe = v.clone();
    effect(move || seen2.borrow_mut().push(probe.get()));

    a.set(2);
    b.set(20);
    assert_eq!(*seen.borrow(), vec![11, 12, 22]);
}

#[test]
fn static_value_does_not_track() {
    // Reading a static `MaybeSignal` inside an effect creates no dependency, so
    // the effect runs exactly once.
    let runs = Rc::new(RefCell::new(0));
    let runs2 = runs.clone();
    let v: MaybeSignal<i32> = MaybeSignal::Static(3);
    effect(move || {
        let _ = v.get();
        *runs2.borrow_mut() += 1;
    });
    assert_eq!(*runs.borrow(), 1);
}

#[test]
#[allow(clippy::useless_conversion)] // deliberately exercising the reflexive `.into()`
fn reflexive_into_is_identity() {
    // A prop already typed `MaybeSignal` must pass through the macro's `.into()`.
    let v: MaybeSignal<i32> = MaybeSignal::Static(9);
    let same: MaybeSignal<i32> = v.into();
    assert_eq!(same.get(), 9);
}

#[test]
fn clone_allows_use_in_multiple_closures() {
    let count = signal(0);
    let v: MaybeSignal<i32> = count.into();
    let a = v.clone();
    let b = v.clone();
    let ea = move || a.get();
    let eb = move || b.get();
    assert_eq!(ea(), 0);
    assert_eq!(eb(), 0);
}

#[test]
fn type_annotation_infers_from_signal_binding() {
    // Sanity: a `Signal<T>` binding converts without turbofish.
    let s: Signal<String> = signal("hi".to_string());
    let v: MaybeSignal<String> = s.into();
    assert_eq!(v.get(), "hi");
}
