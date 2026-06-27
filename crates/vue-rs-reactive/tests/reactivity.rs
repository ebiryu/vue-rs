//! Behavior contract for the reactive core.
//! These tests pin the PUBLIC API and observable semantics so the internal
//! backend can later be swapped (e.g. to an alien-signals style graph) freely.

use std::cell::RefCell;
use std::rc::Rc;

use vue_rs_reactive::{batch, computed, computed_raw, effect, next_tick, signal, signal_raw};

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
fn signal_skips_effect_on_equal_set() {
    let count = signal(0);
    let runs = Rc::new(RefCell::new(0));
    let r = runs.clone();
    effect(move || {
        count.get();
        *r.borrow_mut() += 1;
    });
    assert_eq!(*runs.borrow(), 1);
    count.set(0); // equal value -> no notification (default dedup)
    assert_eq!(*runs.borrow(), 1);
    count.set(1); // changed -> rerun
    assert_eq!(*runs.borrow(), 2);
}

#[test]
fn signal_raw_notifies_even_on_equal_set() {
    let count = signal_raw(0);
    let runs = Rc::new(RefCell::new(0));
    let r = runs.clone();
    effect(move || {
        count.get();
        *r.borrow_mut() += 1;
    });
    assert_eq!(*runs.borrow(), 1);
    count.set(0); // no equality check -> still reruns
    assert_eq!(*runs.borrow(), 2);
}

#[test]
fn computed_skips_downstream_effect_when_value_is_unchanged() {
    // Pull-based + equality: an effect downstream of a computed does NOT re-run
    // when an upstream change leaves the computed's value the same.
    let n = signal(2);
    let parity = computed(move || n.get() % 2);
    let runs = Rc::new(RefCell::new(0));
    let r = runs.clone();
    effect(move || {
        parity.get();
        *r.borrow_mut() += 1;
    });
    assert_eq!(*runs.borrow(), 1);

    n.set(4); // parity 0 -> 0 (unchanged): effect skipped
    assert_eq!(*runs.borrow(), 1);

    n.set(5); // parity 0 -> 1 (changed): effect runs
    assert_eq!(*runs.borrow(), 2);

    n.set(7); // parity 1 -> 1 (unchanged): skipped
    assert_eq!(*runs.borrow(), 2);
}

#[test]
fn computed_raw_reruns_downstream_even_when_value_is_unchanged() {
    let n = signal(2);
    let parity = computed_raw(move || n.get() % 2);
    let runs = Rc::new(RefCell::new(0));
    let r = runs.clone();
    effect(move || {
        parity.get();
        *r.borrow_mut() += 1;
    });
    assert_eq!(*runs.borrow(), 1);
    n.set(4); // parity 0 -> 0, but computed_raw has no dedup -> effect reruns
    assert_eq!(*runs.borrow(), 2);
}

#[test]
fn writing_inside_a_read_closure_does_not_panic() {
    let src = signal(2);
    let dst = signal(0);
    let runs = Rc::new(RefCell::new(0));
    let r = runs.clone();
    effect(move || {
        dst.get();
        *r.borrow_mut() += 1;
    });
    assert_eq!(*runs.borrow(), 1);

    // Mutating reactive state inside a `with` read must not deadlock/panic; the
    // dependent effect runs once the read completes.
    src.with(|v| dst.set(v * 10));
    assert_eq!(dst.get(), 20);
    assert_eq!(*runs.borrow(), 2);
}

#[test]
fn next_tick_runs_immediately_when_idle() {
    let log = Rc::new(RefCell::new(Vec::new()));
    let l = log.clone();
    next_tick(move || l.borrow_mut().push("tick"));
    assert_eq!(*log.borrow(), vec!["tick"]);
}

#[test]
fn next_tick_runs_after_effects_flush() {
    let s = signal(0);
    let log: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
    let l = log.clone();
    effect(move || {
        let v = s.get();
        l.borrow_mut().push(format!("effect {v}"));
    });

    let l2 = log.clone();
    batch(|| {
        s.set(1);
        let l3 = l2.clone();
        next_tick(move || l3.borrow_mut().push("tick".into()));
        l2.borrow_mut().push("in-batch".into());
    });

    // Effects flush before next_tick, both after the synchronous batch body.
    assert_eq!(
        *log.borrow(),
        vec!["effect 0", "in-batch", "effect 1", "tick"]
    );
}

#[test]
fn batch_returns_inner_value() {
    let value = batch(|| 7);
    assert_eq!(value, 7);
}

#[test]
fn wide_fanout_dedups_and_reruns_each_subscriber_once() {
    // A single signal read by many effects, and read multiple times within one
    // effect, must register each dependency edge exactly once: every subscriber
    // reruns exactly once per change, with no duplicate notifications.
    let shared = signal(0);
    let counters: Vec<Rc<RefCell<i32>>> =
        (0..50).map(|_| Rc::new(RefCell::new(0))).collect();
    for counter in &counters {
        let c = counter.clone();
        effect(move || {
            // Read the same signal several times within one run: the duplicate
            // reads must not create duplicate edges.
            let _ = shared.get() + shared.get() + shared.get();
            *c.borrow_mut() += 1;
        });
    }
    for counter in &counters {
        assert_eq!(*counter.borrow(), 1); // initial run
    }

    shared.set(1);
    for counter in &counters {
        assert_eq!(*counter.borrow(), 2); // exactly one rerun, no duplicates
    }

    shared.set(1); // equal value -> dedup, no rerun
    for counter in &counters {
        assert_eq!(*counter.borrow(), 2);
    }
}

#[test]
fn dynamic_retracking_does_not_accumulate_duplicate_edges() {
    // Re-running an effect that re-reads the same source must not grow its edge
    // set: after many reruns the source still reruns the effect exactly once.
    let toggle = signal(true);
    let value = signal(0);
    let runs = Rc::new(RefCell::new(0));
    let r = runs.clone();
    effect(move || {
        // Always reads `value`; `toggle` flips whether `value` is read twice.
        if toggle.get() {
            let _ = value.get();
        }
        let _ = value.get();
        *r.borrow_mut() += 1;
    });
    assert_eq!(*runs.borrow(), 1);

    for i in 0..10 {
        toggle.set(i % 2 == 0); // forces reruns and re-tracking of `value`
    }
    let after_toggles = *runs.borrow();

    value.set(1); // a single change must rerun the effect exactly once
    assert_eq!(*runs.borrow(), after_toggles + 1);
}

#[test]
fn reordered_dependencies_stay_tracked() {
    // Reading the same set of dependencies in a different order across runs must
    // not drop or duplicate any edge: every dependency still triggers exactly one
    // rerun. Exercises link splicing/pruning when deps are reordered.
    let order = signal(true);
    let a = signal(1);
    let b = signal(2);
    let runs = Rc::new(RefCell::new(0));
    let last = Rc::new(RefCell::new((0, 0)));
    let (r, l) = (runs.clone(), last.clone());
    effect(move || {
        // The read order of `a` and `b` flips with `order`.
        let pair = if order.get() {
            (a.get(), b.get())
        } else {
            let y = b.get();
            (a.get(), y)
        };
        *l.borrow_mut() = pair;
        *r.borrow_mut() += 1;
    });
    assert_eq!(*runs.borrow(), 1);
    assert_eq!(*last.borrow(), (1, 2));

    order.set(false); // reorder reads -> rerun, deps unchanged
    assert_eq!(*runs.borrow(), 2);

    // After reordering, both a and b must still each trigger exactly one rerun.
    a.set(10);
    assert_eq!(*runs.borrow(), 3);
    assert_eq!(*last.borrow(), (10, 2));
    b.set(20);
    assert_eq!(*runs.borrow(), 4);
    assert_eq!(*last.borrow(), (10, 20));

    order.set(true); // reorder back
    assert_eq!(*runs.borrow(), 5);
    a.set(11);
    b.set(21);
    assert_eq!(*runs.borrow(), 7);
    assert_eq!(*last.borrow(), (11, 21));
}

#[test]
fn dependency_read_through_a_recomputed_dep_does_not_duplicate() {
    // Read a signal directly, then through a computed that also reads it, then
    // directly again. The interleaved nested tracking must not corrupt edges:
    // one change still reruns the effect exactly once.
    let n = signal(1);
    let doubled = computed(move || n.get() * 2);
    let runs = Rc::new(RefCell::new(0));
    let r = runs.clone();
    effect(move || {
        let _ = n.get();
        let _ = doubled.get(); // recomputes, reading n again under nested tracking
        let _ = n.get();
        *r.borrow_mut() += 1;
    });
    assert_eq!(*runs.borrow(), 1);
    n.set(2);
    assert_eq!(*runs.borrow(), 2); // exactly one rerun, no duplicate notifications
    n.set(3);
    assert_eq!(*runs.borrow(), 3);
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
