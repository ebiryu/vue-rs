//! Behavior contract for `watch`'s flush timing: `pre` (default) runs the
//! callback before post-flush callbacks, `post` defers it until after all
//! effects have run, and `sync` runs it synchronously on every change (even
//! inside a batch). Also covers the general `watch_with` entry point and its
//! `WatchOptions`.

use std::cell::RefCell;
use std::rc::Rc;

use vue_rs_reactive::{batch, effect, signal, watch_with, Flush, WatchOptions};

#[test]
fn default_options_match_lazy_pre_watch() {
    let count = signal(0);
    let calls = Rc::new(RefCell::new(Vec::<(i32, Option<i32>)>::new()));
    let c = calls.clone();
    watch_with(
        move || count.get(),
        move |new, old| c.borrow_mut().push((*new, old.copied())),
        WatchOptions::default(),
    );

    assert!(calls.borrow().is_empty(), "does not fire on setup");
    count.set(1);
    assert_eq!(*calls.borrow(), vec![(1, Some(0))]);
}

#[test]
fn immediate_option_fires_on_setup_with_no_old() {
    let count = signal(7);
    let calls = Rc::new(RefCell::new(Vec::<(i32, Option<i32>)>::new()));
    let c = calls.clone();
    watch_with(
        move || count.get(),
        move |new, old| c.borrow_mut().push((*new, old.copied())),
        WatchOptions {
            immediate: true,
            ..Default::default()
        },
    );

    assert_eq!(*calls.borrow(), vec![(7, None)]);
    count.set(8);
    assert_eq!(*calls.borrow(), vec![(7, None), (8, Some(7))]);
}

#[test]
fn post_flush_runs_the_callback_after_effects() {
    let count = signal(0);
    let log = Rc::new(RefCell::new(Vec::<&'static str>::new()));

    // The post-flush watcher is registered FIRST, so under `pre` its callback
    // would run before the effect below. Under `post` it must run afterwards.
    let l = log.clone();
    watch_with(
        move || count.get(),
        move |_, _| l.borrow_mut().push("watch"),
        WatchOptions {
            flush: Flush::Post,
            ..Default::default()
        },
    );

    let l = log.clone();
    effect(move || {
        count.get();
        l.borrow_mut().push("effect");
    });

    log.borrow_mut().clear(); // drop the effect's initial run
    count.set(1);
    assert_eq!(*log.borrow(), vec!["effect", "watch"]);
}

#[test]
fn post_flush_coalesces_batched_writes_into_one_call() {
    let count = signal(0);
    let calls = Rc::new(RefCell::new(Vec::<(i32, Option<i32>)>::new()));
    let c = calls.clone();
    watch_with(
        move || count.get(),
        move |new, old| c.borrow_mut().push((*new, old.copied())),
        WatchOptions {
            flush: Flush::Post,
            ..Default::default()
        },
    );

    batch(|| {
        count.set(1);
        count.set(2);
        count.set(3);
    });
    assert_eq!(*calls.borrow(), vec![(3, Some(0))]);
}

#[test]
fn sync_flush_fires_on_every_change_inside_a_batch() {
    let count = signal(0);
    let calls = Rc::new(RefCell::new(Vec::<(i32, Option<i32>)>::new()));
    let c = calls.clone();
    watch_with(
        move || count.get(),
        move |new, old| c.borrow_mut().push((*new, old.copied())),
        WatchOptions {
            flush: Flush::Sync,
            ..Default::default()
        },
    );

    batch(|| {
        count.set(1);
        count.set(2);
        count.set(3);
        // A sync watcher has already fired for each individual write.
        assert_eq!(
            *c_snapshot(&calls),
            vec![(1, Some(0)), (2, Some(1)), (3, Some(2))]
        );
    });
    assert_eq!(
        *calls.borrow(),
        vec![(1, Some(0)), (2, Some(1)), (3, Some(2))]
    );
}

#[test]
fn sync_flush_still_dedups_unchanged_values() {
    let count = signal(0);
    let calls = Rc::new(RefCell::new(0));
    let c = calls.clone();
    watch_with(
        move || count.get(),
        move |_, _| *c.borrow_mut() += 1,
        WatchOptions {
            flush: Flush::Sync,
            ..Default::default()
        },
    );

    count.set(0); // no change
    assert_eq!(*calls.borrow(), 0);
    count.set(1);
    assert_eq!(*calls.borrow(), 1);
}

fn c_snapshot<'a, T>(cell: &'a Rc<RefCell<T>>) -> std::cell::Ref<'a, T> {
    cell.borrow()
}
