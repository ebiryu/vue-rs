//! Behavior contract for the writable computed (Vue's `computed({ get, set })`).
//! A `writable_computed` reads like a memoized derived value and, when set, runs
//! a setter that typically writes back to upstream signals.

use std::cell::RefCell;
use std::rc::Rc;

use vue_rs_reactive::{computed, effect, signal, writable_computed};

#[test]
fn reads_the_getter_like_a_computed() {
    let first = signal(String::from("Jane"));
    let last = signal(String::from("Doe"));
    let full = writable_computed(
        move || format!("{} {}", first.get(), last.get()),
        move |_v: String| {},
    );
    assert_eq!(full.get(), "Jane Doe");
}

#[test]
fn set_runs_the_setter_which_writes_upstream() {
    let first = signal(String::from("Jane"));
    let last = signal(String::from("Doe"));
    let full = writable_computed(
        move || format!("{} {}", first.get(), last.get()),
        move |v: String| {
            let (f, l) = v.split_once(' ').unwrap_or((v.as_str(), ""));
            first.set(f.to_string());
            last.set(l.to_string());
        },
    );

    full.set(String::from("John Smith"));

    assert_eq!(first.get(), "John Smith".split(' ').next().unwrap());
    assert_eq!(last.get(), "Smith");
    assert_eq!(full.get(), "John Smith");
}

#[test]
fn recomputes_when_an_upstream_dependency_changes() {
    let n = signal(1);
    let doubled = writable_computed(move || n.get() * 2, move |v: i32| n.set(v / 2));
    assert_eq!(doubled.get(), 2);

    n.set(5);
    assert_eq!(doubled.get(), 10);
}

#[test]
fn setting_propagates_to_dependent_effects() {
    let n = signal(2);
    let doubled = writable_computed(move || n.get() * 2, move |v: i32| n.set(v / 2));

    let seen = Rc::new(RefCell::new(Vec::new()));
    let seen2 = seen.clone();
    effect(move || seen2.borrow_mut().push(doubled.get()));
    assert_eq!(*seen.borrow(), vec![4]);

    // Writing through the setter updates the upstream signal, which re-derives
    // the computed and re-runs the effect.
    doubled.set(20);
    assert_eq!(*seen.borrow(), vec![4, 20]);
}

#[test]
fn read_dedups_unchanged_value() {
    // A writable computed is `PartialEq`-deduped like `computed`: a recompute to
    // an equal value does not re-run dependents.
    let n = signal(1);
    let parity = writable_computed(move || n.get() % 2, move |_v: i32| {});

    let runs = Rc::new(RefCell::new(0));
    let runs2 = runs.clone();
    effect(move || {
        let _ = parity.get();
        *runs2.borrow_mut() += 1;
    });
    assert_eq!(*runs.borrow(), 1);

    n.set(3); // parity stays 1 -> dependents are not re-run
    assert_eq!(*runs.borrow(), 1);

    n.set(2); // parity flips to 0 -> dependents re-run
    assert_eq!(*runs.borrow(), 2);
}

#[test]
fn can_be_read_by_a_plain_computed() {
    let n = signal(3);
    let w = writable_computed(move || n.get() + 1, move |v: i32| n.set(v - 1));
    let downstream = computed(move || w.get() * 10);
    assert_eq!(downstream.get(), 40);

    w.set(9); // n becomes 8, w becomes 9, downstream becomes 90
    assert_eq!(n.get(), 8);
    assert_eq!(downstream.get(), 90);
}
