//! Contract: ownership scopes, disposal, and effect cleanup.

use std::cell::RefCell;
use std::rc::Rc;

use vue_rs_reactive::{create_root, effect, on_cleanup, signal};

#[test]
fn on_cleanup_runs_before_effect_reruns() {
    let count = signal(0);
    let log: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
    {
        let log = log.clone();
        effect(move || {
            let v = count.get();
            log.borrow_mut().push(format!("run {v}"));
            let log2 = log.clone();
            on_cleanup(move || log2.borrow_mut().push(format!("cleanup {v}")));
        });
    }
    assert_eq!(*log.borrow(), vec!["run 0"]);
    count.set(1);
    // cleanup for the previous run fires before the new run
    assert_eq!(*log.borrow(), vec!["run 0", "cleanup 0", "run 1"]);
}

#[test]
fn nested_effect_is_disposed_when_parent_reruns() {
    let outer = signal(0);
    let inner = signal(0);
    let runs = Rc::new(RefCell::new(0));

    let r = runs.clone();
    effect(move || {
        outer.get(); // parent depends on `outer`
        let r2 = r.clone();
        effect(move || {
            inner.get(); // child depends on `inner`
            *r2.borrow_mut() += 1;
        });
    });

    assert_eq!(*runs.borrow(), 1); // child #1 ran once
    inner.set(1);
    assert_eq!(*runs.borrow(), 2); // child #1 reran

    outer.set(1); // parent reruns -> disposes child #1, creates child #2 (runs once)
    assert_eq!(*runs.borrow(), 3);

    inner.set(2); // only child #2 is alive now -> exactly one more run
    assert_eq!(*runs.borrow(), 4);
}

#[test]
fn create_root_dispose_stops_effects() {
    let count = signal(0);
    let runs = Rc::new(RefCell::new(0));

    let r = runs.clone();
    let disposer = create_root(move || {
        effect(move || {
            count.get();
            *r.borrow_mut() += 1;
        });
    });

    assert_eq!(*runs.borrow(), 1);
    count.set(1);
    assert_eq!(*runs.borrow(), 2);

    disposer.dispose();
    count.set(2); // effect disposed -> no more runs
    assert_eq!(*runs.borrow(), 2);
}

#[test]
fn dispose_runs_pending_cleanups() {
    let cleaned = Rc::new(RefCell::new(false));
    let c = cleaned.clone();
    let disposer = create_root(move || {
        effect(move || {
            let c2 = c.clone();
            on_cleanup(move || *c2.borrow_mut() = true);
        });
    });
    assert!(!*cleaned.borrow());
    disposer.dispose();
    assert!(*cleaned.borrow());
}
