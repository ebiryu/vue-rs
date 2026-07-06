//! Contract: `effect_scope` — Vue's `effectScope()` API shape. Unlike
//! `create_root` (which runs its body immediately and returns a disposer), a
//! scope separates creation, running, and stopping: create the scope, call
//! `run` (possibly several times) to collect effects into it, then `stop` to
//! dispose everything at once.

use std::cell::RefCell;
use std::rc::Rc;

use vue_rs_reactive::{
    effect, effect_scope, effect_scope_detached, get_current_scope, on_cleanup, on_scope_dispose,
    signal,
};

#[test]
fn run_returns_the_bodys_value() {
    let scope = effect_scope();
    let out = scope.run(|| 41 + 1);
    assert_eq!(out, Some(42));
}

#[test]
fn stop_disposes_effects_created_in_run() {
    let count = signal(0);
    let runs = Rc::new(RefCell::new(0));

    let scope = effect_scope();
    let r = runs.clone();
    scope.run(|| {
        effect(move || {
            count.get();
            *r.borrow_mut() += 1;
        });
    });

    assert_eq!(*runs.borrow(), 1);
    count.set(1);
    assert_eq!(*runs.borrow(), 2);

    scope.stop();
    count.set(2); // effect disposed -> no more runs
    assert_eq!(*runs.borrow(), 2);
}

#[test]
fn effects_created_outside_run_are_not_captured() {
    let count = signal(0);
    let runs = Rc::new(RefCell::new(0));

    let scope = effect_scope();
    // Effect created OUTSIDE the scope's run — not owned by the scope.
    let r = runs.clone();
    effect(move || {
        count.get();
        *r.borrow_mut() += 1;
    });

    assert_eq!(*runs.borrow(), 1);
    scope.stop(); // must not stop the outside effect
    count.set(1);
    assert_eq!(*runs.borrow(), 2);
}

#[test]
fn run_accumulates_effects_across_multiple_calls() {
    let count = signal(0);
    let runs = Rc::new(RefCell::new(0));

    let scope = effect_scope();
    for _ in 0..2 {
        let r = runs.clone();
        scope.run(|| {
            effect(move || {
                count.get();
                *r.borrow_mut() += 1;
            });
        });
    }

    assert_eq!(*runs.borrow(), 2); // two effects, each ran once
    scope.stop();
    count.set(1); // both disposed
    assert_eq!(*runs.borrow(), 2);
}

#[test]
fn run_after_stop_is_a_noop_returning_none() {
    let count = signal(0);
    let runs = Rc::new(RefCell::new(0));

    let scope = effect_scope();
    scope.stop();

    let r = runs.clone();
    let out = scope.run(|| {
        effect(move || {
            count.get();
            *r.borrow_mut() += 1;
        });
        7
    });

    assert_eq!(out, None); // scope inactive
    assert_eq!(*runs.borrow(), 0); // body did not run
}

#[test]
fn stop_is_idempotent() {
    let scope = effect_scope();
    scope.run(|| {});
    scope.stop();
    scope.stop(); // second stop must not panic
}

#[test]
fn nested_scope_is_stopped_with_its_parent() {
    let count = signal(0);
    let runs = Rc::new(RefCell::new(0));

    let parent = effect_scope();
    parent.run(|| {
        // A child scope created inside the parent's run is owned by the parent.
        let child = effect_scope();
        let r = runs.clone();
        child.run(|| {
            effect(move || {
                count.get();
                *r.borrow_mut() += 1;
            });
        });
    });

    assert_eq!(*runs.borrow(), 1);
    count.set(1);
    assert_eq!(*runs.borrow(), 2);

    parent.stop(); // stopping the parent stops the nested child too
    count.set(2);
    assert_eq!(*runs.borrow(), 2);
}

#[test]
fn detached_scope_survives_its_creating_scope() {
    let count = signal(0);
    let runs = Rc::new(RefCell::new(0));

    let child = Rc::new(RefCell::new(None));

    let parent = effect_scope();
    let child_slot = child.clone();
    parent.run(|| {
        // A detached scope is NOT owned by the parent.
        let c = effect_scope_detached();
        let r = runs.clone();
        c.run(|| {
            effect(move || {
                count.get();
                *r.borrow_mut() += 1;
            });
        });
        *child_slot.borrow_mut() = Some(c);
    });

    assert_eq!(*runs.borrow(), 1);
    parent.stop(); // does NOT dispose the detached child
    count.set(1);
    assert_eq!(*runs.borrow(), 2); // detached effect still alive

    child.borrow().as_ref().unwrap().stop();
    count.set(2);
    assert_eq!(*runs.borrow(), 2); // now disposed
}

#[test]
fn on_scope_dispose_fires_once_when_the_scope_stops() {
    let disposed = Rc::new(RefCell::new(0));

    let scope = effect_scope();
    let d = disposed.clone();
    scope.run(|| {
        on_scope_dispose(move || *d.borrow_mut() += 1);
    });

    assert_eq!(*disposed.borrow(), 0); // not yet
    scope.stop();
    assert_eq!(*disposed.borrow(), 1);
    scope.stop(); // idempotent — does not fire again
    assert_eq!(*disposed.borrow(), 1);
}

#[test]
fn on_scope_dispose_targets_the_scope_not_the_re_running_effect() {
    // A scope-dispose callback registered at setup fires once, at stop — it is
    // not tied to an effect's re-run cleanup. Contrast `on_cleanup` registered
    // inside the effect body, which fires before each re-run.
    let count = signal(0);
    let scope_log = Rc::new(RefCell::new(0));
    let cleanup_log = Rc::new(RefCell::new(0));

    let scope = effect_scope();
    let s = scope_log.clone();
    let c = cleanup_log.clone();
    scope.run(|| {
        // Registered once, directly in the scope body.
        on_scope_dispose(move || *s.borrow_mut() += 1);
        effect(move || {
            count.get();
            let c2 = c.clone();
            on_cleanup(move || *c2.borrow_mut() += 1);
        });
    });

    count.set(1); // effect re-runs
    count.set(2); // effect re-runs again
                  // on_cleanup fired before each re-run (twice); scope callback not yet.
    assert_eq!(*cleanup_log.borrow(), 2);
    assert_eq!(*scope_log.borrow(), 0);

    scope.stop();
    // Final on_cleanup runs at disposal too (3 total); scope callback fires once.
    assert_eq!(*cleanup_log.borrow(), 3);
    assert_eq!(*scope_log.borrow(), 1);
}

#[test]
fn get_current_scope_is_some_inside_run_and_none_outside() {
    assert!(get_current_scope().is_none());

    let scope = effect_scope();
    let inside = scope.run(|| get_current_scope().is_some());
    assert_eq!(inside, Some(true));

    assert!(get_current_scope().is_none()); // restored after run
}

#[test]
fn get_current_scope_can_stop_the_active_scope() {
    let count = signal(0);
    let runs = Rc::new(RefCell::new(0));

    let scope = effect_scope();
    let handle = scope.run(|| get_current_scope().unwrap());
    let r = runs.clone();
    scope.run(|| {
        effect(move || {
            count.get();
            *r.borrow_mut() += 1;
        });
    });

    assert_eq!(*runs.borrow(), 1);
    handle.unwrap().stop(); // same underlying scope
    count.set(1);
    assert_eq!(*runs.borrow(), 1); // stopped
}
