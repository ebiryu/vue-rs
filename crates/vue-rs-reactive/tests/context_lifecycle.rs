//! Contract: context propagation, scopes, and mount lifecycle.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use vue_rs_reactive::{
    create_root, create_root_detached, effect, flush_mounted, on_mounted, on_unmounted,
    provide_context, run_in_child_scope, signal, use_context, RootDisposer,
};

#[test]
fn context_reaches_descendant_scope() {
    create_root(|| {
        provide_context(42i32);
        run_in_child_scope(|| {
            assert_eq!(use_context::<i32>(), Some(42));
        });
    })
    .dispose();
}

#[test]
fn nearest_provider_wins() {
    create_root(|| {
        provide_context(1i32);
        run_in_child_scope(|| {
            provide_context(2i32);
            run_in_child_scope(|| {
                assert_eq!(use_context::<i32>(), Some(2));
            });
        });
    })
    .dispose();
}

#[test]
fn missing_context_is_none() {
    create_root(|| {
        assert_eq!(use_context::<String>(), None);
    })
    .dispose();
}

#[test]
fn context_is_keyed_by_type() {
    create_root(|| {
        provide_context(7i32);
        provide_context(String::from("hi"));
        run_in_child_scope(|| {
            assert_eq!(use_context::<i32>(), Some(7));
            assert_eq!(use_context::<String>(), Some(String::from("hi")));
        });
    })
    .dispose();
}

#[test]
fn child_scope_is_disposed_with_parent() {
    let s = signal(0); // created outside the root so it outlives it
    let runs = Rc::new(Cell::new(0));
    let r = runs.clone();

    let root = create_root(|| {
        run_in_child_scope(|| {
            effect(move || {
                s.get();
                r.set(r.get() + 1);
            });
        });
    });

    assert_eq!(runs.get(), 1);
    s.set(1);
    assert_eq!(runs.get(), 2); // effect alive

    root.dispose(); // disposes the child scope and its effect
    s.set(2);
    assert_eq!(runs.get(), 2); // no more runs
}

#[test]
fn detached_root_survives_parent_dispose() {
    let s = signal(0);
    let runs = Rc::new(Cell::new(0));
    let r = runs.clone();
    let holder: Rc<RefCell<Option<RootDisposer>>> = Rc::new(RefCell::new(None));
    let h = holder.clone();

    let parent = create_root(|| {
        let detached = create_root_detached(move || {
            effect(move || {
                s.get();
                r.set(r.get() + 1);
            });
        });
        *h.borrow_mut() = Some(detached);
    });

    assert_eq!(runs.get(), 1);
    parent.dispose(); // detached is NOT owned by parent
    s.set(1);
    assert_eq!(runs.get(), 2); // still alive

    holder.borrow_mut().take().unwrap().dispose();
    s.set(2);
    assert_eq!(runs.get(), 2); // now disposed
}

#[test]
fn on_unmounted_runs_on_scope_dispose() {
    let fired = Rc::new(Cell::new(false));
    let f = fired.clone();
    let root = create_root(|| {
        run_in_child_scope(|| {
            on_unmounted(move || f.set(true));
        });
    });
    assert!(!fired.get());
    root.dispose();
    assert!(fired.get());
}

#[test]
fn on_mounted_runs_once_on_flush() {
    let log: Rc<RefCell<Vec<i32>>> = Rc::new(RefCell::new(Vec::new()));
    let l = log.clone();
    on_mounted(move || l.borrow_mut().push(1));

    assert!(log.borrow().is_empty()); // not run before flush
    flush_mounted();
    assert_eq!(*log.borrow(), vec![1]);
    flush_mounted(); // already drained
    assert_eq!(*log.borrow(), vec![1]);
}

#[test]
fn mounted_callbacks_run_in_registration_order() {
    let log: Rc<RefCell<Vec<i32>>> = Rc::new(RefCell::new(Vec::new()));
    for n in [1, 2, 3] {
        let l = log.clone();
        on_mounted(move || l.borrow_mut().push(n));
    }
    flush_mounted();
    assert_eq!(*log.borrow(), vec![1, 2, 3]);
}
