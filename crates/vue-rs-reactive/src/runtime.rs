//! Arena-based fine-grained reactive graph.
//!
//! - A thread-local arena of nodes keyed by a monotonic id; handles are `Copy`.
//! - Computeds are lazy: recomputed on read when marked dirty.
//! - On a write, subscribers are walked: downstream computeds are marked dirty
//!   and each affected effect is collected once (diamond-safe), then run.
//! - Dependencies are dynamic: a node's sources are cleared before every re-run.
//! - Ownership: each node records the owner active at creation. When an owner
//!   re-runs or is disposed, its owned children are disposed and its registered
//!   cleanups run.

use std::any::Any;
use std::cell::RefCell;
use std::collections::HashMap;
use std::marker::PhantomData;

type NodeId = usize;

enum Kind {
    Signal,
    Computed {
        compute: Option<Box<dyn Fn() -> Box<dyn Any>>>,
        dirty: bool,
    },
    Effect {
        run: Option<Box<dyn FnMut()>>,
    },
    /// An ownership-only node created by [`create_root`]; not reactive.
    Root,
}

struct Node {
    value: Option<Box<dyn Any>>,
    sources: Vec<NodeId>,     // nodes this node reads (its dependencies)
    subscribers: Vec<NodeId>, // nodes that read this node (its dependents)
    owner: Option<NodeId>,    // owner active when this node was created
    owned: Vec<NodeId>,       // nodes created while this node was the owner
    cleanups: Vec<Box<dyn FnOnce()>>,
    kind: Kind,
}

struct Runtime {
    nodes: HashMap<NodeId, Node>,
    next_id: NodeId,
    observer: Option<NodeId>, // current dependency-tracking node
    owner: Option<NodeId>,    // current ownership node
}

thread_local! {
    static RT: RefCell<Runtime> = RefCell::new(Runtime {
        nodes: HashMap::new(),
        next_id: 0,
        observer: None,
        owner: None,
    });
}

fn new_node(kind: Kind, value: Option<Box<dyn Any>>) -> NodeId {
    RT.with_borrow_mut(|rt| {
        let id = rt.next_id;
        rt.next_id += 1;
        let owner = rt.owner;
        rt.nodes.insert(
            id,
            Node {
                value,
                sources: Vec::new(),
                subscribers: Vec::new(),
                owner,
                owned: Vec::new(),
                cleanups: Vec::new(),
                kind,
            },
        );
        if let Some(owner) = owner {
            if let Some(node) = rt.nodes.get_mut(&owner) {
                node.owned.push(id);
            }
        }
        id
    })
}

/// Record that the currently running observer depends on `source`.
fn record_dependency(source: NodeId) {
    RT.with_borrow_mut(|rt| {
        if let Some(obs) = rt.observer {
            if let Some(node) = rt.nodes.get_mut(&obs) {
                if !node.sources.contains(&source) {
                    node.sources.push(source);
                }
            }
            if let Some(node) = rt.nodes.get_mut(&source) {
                if !node.subscribers.contains(&obs) {
                    node.subscribers.push(obs);
                }
            }
        }
    });
}

/// Drop all of `id`'s current dependency edges.
fn clear_sources(id: NodeId) {
    let sources = RT.with_borrow_mut(|rt| {
        rt.nodes
            .get_mut(&id)
            .map(|n| std::mem::take(&mut n.sources))
            .unwrap_or_default()
    });
    RT.with_borrow_mut(|rt| {
        for s in sources {
            if let Some(node) = rt.nodes.get_mut(&s) {
                node.subscribers.retain(|x| *x != id);
            }
        }
    });
}

/// Dispose owned children and run pending cleanups, keeping `id` alive so it can
/// re-run. Also clears `id`'s dependency edges.
fn prepare_rerun(id: NodeId) {
    let owned = RT.with_borrow_mut(|rt| {
        rt.nodes
            .get_mut(&id)
            .map(|n| std::mem::take(&mut n.owned))
            .unwrap_or_default()
    });
    for child in owned {
        dispose_node(child);
    }
    let cleanups = RT.with_borrow_mut(|rt| {
        rt.nodes
            .get_mut(&id)
            .map(|n| std::mem::take(&mut n.cleanups))
            .unwrap_or_default()
    });
    for cleanup in cleanups.into_iter().rev() {
        cleanup();
    }
    clear_sources(id);
}

/// Fully remove a node: dispose its children, run its cleanups, detach all edges.
fn dispose_node(id: NodeId) {
    if RT.with_borrow(|rt| !rt.nodes.contains_key(&id)) {
        return;
    }
    prepare_rerun(id); // dispose children, run cleanups, drop source edges
    RT.with_borrow_mut(|rt| {
        // Detach from any subscribers that still point at this node.
        let subscribers = rt
            .nodes
            .get_mut(&id)
            .map(|n| std::mem::take(&mut n.subscribers))
            .unwrap_or_default();
        for s in subscribers {
            if let Some(node) = rt.nodes.get_mut(&s) {
                node.sources.retain(|x| *x != id);
            }
        }
        // Detach from owner's owned list.
        if let Some(owner) = rt.nodes.get(&id).and_then(|n| n.owner) {
            if let Some(node) = rt.nodes.get_mut(&owner) {
                node.owned.retain(|x| *x != id);
            }
        }
        rt.nodes.remove(&id);
    });
}

fn ensure_fresh(id: NodeId) {
    let needs = RT.with_borrow(|rt| {
        matches!(
            rt.nodes.get(&id).map(|n| &n.kind),
            Some(Kind::Computed { dirty: true, .. })
        )
    });
    if needs {
        recompute(id);
    }
}

/// Run `id`'s reactive body with `id` installed as observer and owner.
fn run_scoped(id: NodeId, body: impl FnOnce()) {
    let (prev_obs, prev_owner) = RT.with_borrow_mut(|rt| {
        let p = (rt.observer, rt.owner);
        rt.observer = Some(id);
        rt.owner = Some(id);
        p
    });
    body();
    RT.with_borrow_mut(|rt| {
        rt.observer = prev_obs;
        rt.owner = prev_owner;
    });
}

fn recompute(id: NodeId) {
    let compute = RT.with_borrow_mut(|rt| match rt.nodes.get_mut(&id).map(|n| &mut n.kind) {
        Some(Kind::Computed { compute, .. }) => compute.take().expect("reentrant compute"),
        _ => unreachable!("recompute on non-computed node"),
    });
    prepare_rerun(id);
    let mut value = None;
    run_scoped(id, || value = Some(compute()));
    RT.with_borrow_mut(|rt| {
        if let Some(node) = rt.nodes.get_mut(&id) {
            if let Kind::Computed { compute: c, dirty } = &mut node.kind {
                *c = Some(compute);
                *dirty = false;
            }
            node.value = value;
        }
    });
}

fn run_effect(id: NodeId) {
    // Returns None if the node was disposed (e.g. mid-propagation) -> skip.
    let run = RT.with_borrow_mut(|rt| match rt.nodes.get_mut(&id).map(|n| &mut n.kind) {
        Some(Kind::Effect { run }) => run.take(),
        _ => None,
    });
    let Some(mut run) = run else { return };
    prepare_rerun(id);
    // borrow `run` so it can be stored back afterwards (can't move it into the scope)
    #[allow(clippy::redundant_closure)]
    run_scoped(id, || run());
    RT.with_borrow_mut(|rt| {
        if let Some(Kind::Effect { run: r }) = rt.nodes.get_mut(&id).map(|n| &mut n.kind) {
            *r = Some(run);
        }
    });
}

/// Propagate a write from `start`: mark downstream computeds dirty and run each
/// affected effect exactly once.
fn propagate(start: NodeId) {
    enum Act {
        MarkComputed,
        Effect,
        None,
    }
    let mut to_run: Vec<NodeId> = Vec::new();
    let mut stack: Vec<NodeId> =
        RT.with_borrow(|rt| rt.nodes.get(&start).map(|n| n.subscribers.clone()).unwrap_or_default());
    while let Some(n) = stack.pop() {
        let act = RT.with_borrow(|rt| match rt.nodes.get(&n).map(|node| &node.kind) {
            Some(Kind::Computed { dirty, .. }) => {
                if *dirty {
                    Act::None
                } else {
                    Act::MarkComputed
                }
            }
            Some(Kind::Effect { .. }) => Act::Effect,
            _ => Act::None,
        });
        match act {
            Act::MarkComputed => {
                let subs = RT.with_borrow_mut(|rt| {
                    if let Some(Kind::Computed { dirty, .. }) =
                        rt.nodes.get_mut(&n).map(|node| &mut node.kind)
                    {
                        *dirty = true;
                    }
                    rt.nodes.get(&n).map(|node| node.subscribers.clone()).unwrap_or_default()
                });
                stack.extend(subs);
            }
            Act::Effect => {
                if !to_run.contains(&n) {
                    to_run.push(n);
                }
            }
            Act::None => {}
        }
    }
    for e in to_run {
        run_effect(e);
    }
}

fn read_with<T: 'static, R>(id: NodeId, f: impl FnOnce(&T) -> R) -> R {
    record_dependency(id);
    ensure_fresh(id);
    RT.with_borrow(|rt| {
        let any = rt.nodes[&id].value.as_ref().expect("node has no value");
        let v = any
            .downcast_ref::<T>()
            .expect("type mismatch reading reactive node");
        f(v)
    })
}

fn set_value<T: 'static>(id: NodeId, v: T) {
    RT.with_borrow_mut(|rt| {
        if let Some(node) = rt.nodes.get_mut(&id) {
            node.value = Some(Box::new(v));
        }
    });
    propagate(id);
}

fn update_value<T: 'static>(id: NodeId, f: impl FnOnce(&mut T)) {
    RT.with_borrow_mut(|rt| {
        let any = rt.nodes.get_mut(&id).and_then(|n| n.value.as_mut()).expect("node has no value");
        let v = any
            .downcast_mut::<T>()
            .expect("type mismatch updating reactive node");
        f(v);
    });
    propagate(id);
}

// ---------------------------------------------------------------------------
// Public handles, constructors, and scope helpers
// ---------------------------------------------------------------------------

/// A writable reactive value. Vue's `ref(...)`; named `signal` because `ref`
/// is a Rust keyword (the SFC compiler maps `ref` -> `signal`).
pub struct Signal<T> {
    id: NodeId,
    _t: PhantomData<T>,
}

impl<T> Clone for Signal<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T> Copy for Signal<T> {}

/// A memoized derived value. Vue's `computed(...)`.
pub struct Memo<T> {
    id: NodeId,
    _t: PhantomData<T>,
}

impl<T> Clone for Memo<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T> Copy for Memo<T> {}

/// Create a writable reactive value.
pub fn signal<T: 'static>(value: T) -> Signal<T> {
    Signal {
        id: new_node(Kind::Signal, Some(Box::new(value))),
        _t: PhantomData,
    }
}

impl<T: 'static> Signal<T> {
    /// Read with dependency tracking, cloning out the value.
    pub fn get(self) -> T
    where
        T: Clone,
    {
        self.with(|v| v.clone())
    }

    /// Read with dependency tracking, without requiring `Clone`.
    pub fn with<R>(self, f: impl FnOnce(&T) -> R) -> R {
        read_with::<T, R>(self.id, f)
    }

    /// Replace the value and notify dependents.
    pub fn set(self, value: T) {
        set_value(self.id, value);
    }

    /// Mutate the value in place and notify dependents.
    pub fn update(self, f: impl FnOnce(&mut T)) {
        update_value(self.id, f);
    }
}

/// Create a memoized derived value.
pub fn computed<T: 'static>(f: impl Fn() -> T + 'static) -> Memo<T> {
    let compute: Box<dyn Fn() -> Box<dyn Any>> = Box::new(move || Box::new(f()) as Box<dyn Any>);
    Memo {
        id: new_node(
            Kind::Computed {
                compute: Some(compute),
                dirty: true,
            },
            None,
        ),
        _t: PhantomData,
    }
}

impl<T: 'static> Memo<T> {
    /// Read with dependency tracking, cloning out the value.
    pub fn get(self) -> T
    where
        T: Clone,
    {
        self.with(|v| v.clone())
    }

    /// Read with dependency tracking, without requiring `Clone`.
    pub fn with<R>(self, f: impl FnOnce(&T) -> R) -> R {
        read_with::<T, R>(self.id, f)
    }
}

/// Run `f` immediately and re-run it whenever any reactive value it read changes.
pub fn effect(f: impl FnMut() + 'static) {
    let id = new_node(
        Kind::Effect {
            run: Some(Box::new(f)),
        },
        None,
    );
    run_effect(id);
}

/// Register a cleanup to run before the current owner re-runs, and when it is
/// disposed. No-op outside a reactive scope.
pub fn on_cleanup(f: impl FnOnce() + 'static) {
    RT.with_borrow_mut(|rt| {
        if let Some(owner) = rt.owner {
            if let Some(node) = rt.nodes.get_mut(&owner) {
                node.cleanups.push(Box::new(f));
            }
        }
    });
}

/// Handle returned by [`create_root`]; disposes everything created in the root.
pub struct RootDisposer {
    id: NodeId,
}

impl RootDisposer {
    /// Dispose the root and all reactive nodes created within it.
    pub fn dispose(self) {
        dispose_node(self.id);
    }
}

/// Run `f` inside a fresh ownership scope. Reactive nodes created within are
/// owned by the root and torn down when the returned [`RootDisposer`] is disposed.
pub fn create_root(f: impl FnOnce()) -> RootDisposer {
    let id = new_node(Kind::Root, None);
    let (prev_obs, prev_owner) = RT.with_borrow_mut(|rt| {
        let p = (rt.observer, rt.owner);
        rt.observer = None; // root body itself is not reactive
        rt.owner = Some(id);
        p
    });
    f();
    RT.with_borrow_mut(|rt| {
        rt.observer = prev_obs;
        rt.owner = prev_owner;
    });
    RootDisposer { id }
}
