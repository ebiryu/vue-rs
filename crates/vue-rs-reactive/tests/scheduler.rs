//! Async scheduler contract.
//!
//! By default effects flush synchronously. When an async scheduler is installed
//! (e.g. backed by `queueMicrotask` in the browser), synchronous writes are
//! coalesced and the flush is deferred until the host drains it via
//! [`flush_jobs`]. These tests drive the flush by hand to keep them native.

use std::cell::RefCell;
use std::rc::Rc;

use vue_rs_reactive::{
    clear_scheduler, effect, flush_jobs, next_tick, set_scheduler, signal,
};

#[test]
fn scheduler_defers_effect_until_flush_jobs() {
    let scheduled = Rc::new(RefCell::new(0usize));
    {
        let s = scheduled.clone();
        set_scheduler(move || *s.borrow_mut() += 1);
    }

    let log = Rc::new(RefCell::new(Vec::new()));
    let count = signal(0);
    {
        let l = log.clone();
        effect(move || l.borrow_mut().push(count.get()));
    }
    // The initial effect run is synchronous, before any scheduling.
    assert_eq!(*log.borrow(), vec![0]);

    count.set(1);
    count.set(2);
    // No synchronous re-run; the writes only requested a single flush.
    assert_eq!(*log.borrow(), vec![0]);
    assert_eq!(*scheduled.borrow(), 1, "two writes schedule one flush");

    flush_jobs();
    // Coalesced: the effect reruns once with the final value.
    assert_eq!(*log.borrow(), vec![0, 2]);

    clear_scheduler();
}

#[test]
fn scheduler_rearms_after_each_flush() {
    let scheduled = Rc::new(RefCell::new(0usize));
    {
        let s = scheduled.clone();
        set_scheduler(move || *s.borrow_mut() += 1);
    }

    let count = signal(0);
    effect(move || {
        count.get();
    });

    count.set(1);
    assert_eq!(*scheduled.borrow(), 1);
    flush_jobs();

    // A write after the previous flush schedules a fresh flush.
    count.set(2);
    assert_eq!(*scheduled.borrow(), 2);
    flush_jobs();

    clear_scheduler();
}

#[test]
fn next_tick_is_async_under_scheduler_and_runs_after_effects() {
    set_scheduler(|| {}); // flush is driven manually below

    let log = Rc::new(RefCell::new(Vec::<&str>::new()));
    let count = signal(0);
    {
        let l = log.clone();
        effect(move || {
            count.get();
            l.borrow_mut().push("effect");
        });
    }
    log.borrow_mut().clear(); // drop the synchronous initial run

    count.set(1);
    {
        let l = log.clone();
        next_tick(move || l.borrow_mut().push("tick"));
    }
    log.borrow_mut().push("sync-end");

    // Nothing has flushed yet: the synchronous frame completes first.
    assert_eq!(*log.borrow(), vec!["sync-end"]);

    flush_jobs();
    // The effect reruns, then the next_tick callback.
    assert_eq!(*log.borrow(), vec!["sync-end", "effect", "tick"]);

    clear_scheduler();
}

#[test]
fn clearing_scheduler_restores_synchronous_flush() {
    set_scheduler(|| {});
    clear_scheduler();

    let log = Rc::new(RefCell::new(Vec::new()));
    let count = signal(0);
    {
        let l = log.clone();
        effect(move || l.borrow_mut().push(count.get()));
    }
    count.set(5);
    // With no scheduler, the write flushes synchronously.
    assert_eq!(*log.borrow(), vec![0, 5]);
}
