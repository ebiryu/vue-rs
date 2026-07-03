//! Contract for `ReadSignal<T>`: a read-only view of a reactive value. Props
//! flow down as `ReadSignal` so a child can observe but not mutate parent state
//! (one-way data flow); writes go back up through emits.

use std::cell::RefCell;
use std::rc::Rc;

use vue_rs_reactive::{computed, effect, signal, ReadSignal};

#[test]
fn read_signal_reads_the_underlying_signal() {
    let count = signal(3);
    let view: ReadSignal<i32> = count.read_only();
    assert_eq!(view.get(), 3);
}

#[test]
fn read_signal_tracks_updates_to_the_source() {
    let count = signal(0);
    let view = count.read_only();

    let seen = Rc::new(RefCell::new(Vec::new()));
    let seen2 = seen.clone();
    effect(move || seen2.borrow_mut().push(view.get()));

    count.set(1);
    count.set(2);
    assert_eq!(*seen.borrow(), vec![0, 1, 2]);
}

#[test]
fn read_signal_from_signal_via_into() {
    let count = signal(7);
    let view: ReadSignal<i32> = count.into();
    assert_eq!(view.get(), 7);
}

#[test]
fn read_signal_from_memo_via_into() {
    let count = signal(2);
    let doubled = computed(move || count.get() * 2);
    let view: ReadSignal<i32> = doubled.into();
    assert_eq!(view.get(), 4);
    count.set(5);
    assert_eq!(view.get(), 10);
}

#[test]
#[allow(clippy::useless_conversion)] // deliberately exercising the reflexive `.into()`
fn read_signal_reflexive_into_is_identity() {
    let count = signal(1);
    let view = count.read_only();
    // `.into()` onto its own type is the reflexive identity (matters because the
    // component macro wraps every reactive prop in `.into()`, so a prop already
    // typed `ReadSignal` must pass through).
    let same: ReadSignal<i32> = view.into();
    assert_eq!(same.get(), 1);
}
