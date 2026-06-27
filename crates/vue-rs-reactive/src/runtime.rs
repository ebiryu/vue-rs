//! Arena-based fine-grained reactive graph (pull-based, glitch-free).
//!
//! Evaluation follows the "reactively" / alien-signals model:
//! - Each node tracks a [`State`]: `Clean`, `Check` (a source may have changed),
//!   or `Dirty` (a source definitely changed).
//! - A write marks direct subscribers `Dirty` and transitive ones `Check`, and
//!   queues affected effects.
//! - Reading a computed pulls: it verifies its sources and only recomputes if one
//!   actually changed. When a computed's value is unchanged (per its `eq`), it does
//!   NOT dirty its observers, so downstream work is skipped.
//! - Dependency edges are intrusive doubly-linked [`Link`]s drawn from a pool:
//!   each link belongs to both its source's subscriber list and its subscriber's
//!   dependency list. Re-running a node re-tracks in place — links are reused when
//!   the same dependencies are read in the same order, and any left unread are
//!   pruned — so unchanged graphs allocate nothing and disposal unlinks in O(1).
//! - Ownership: each node records the owner active at creation; re-running or
//!   disposing an owner disposes its owned children and runs its cleanups.

use std::any::{Any, TypeId};
use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::marker::PhantomData;
use std::rc::Rc;

/// A generational handle into the [`Arena`]. The generation guards against a
/// stale handle accidentally referring to a reused slot.
#[derive(Clone, Copy, PartialEq, Eq)]
struct NodeId {
    index: usize,
    generation: u32,
}

struct Slot {
    generation: u32,
    node: Option<Node>,
}

/// A generational arena: disposed nodes free their slot for reuse, and reads via
/// a stale handle return `None` (generation mismatch) rather than a wrong node.
struct Arena {
    slots: Vec<Slot>,
    free: Vec<usize>,
}

impl Arena {
    fn new() -> Self {
        Self {
            slots: Vec::new(),
            free: Vec::new(),
        }
    }

    fn insert(&mut self, node: Node) -> NodeId {
        if let Some(index) = self.free.pop() {
            let slot = &mut self.slots[index];
            slot.node = Some(node);
            NodeId {
                index,
                generation: slot.generation,
            }
        } else {
            let index = self.slots.len();
            self.slots.push(Slot {
                generation: 0,
                node: Some(node),
            });
            NodeId {
                index,
                generation: 0,
            }
        }
    }

    fn get(&self, id: NodeId) -> Option<&Node> {
        self.slots
            .get(id.index)
            .filter(|s| s.generation == id.generation)
            .and_then(|s| s.node.as_ref())
    }

    fn get_mut(&mut self, id: NodeId) -> Option<&mut Node> {
        self.slots
            .get_mut(id.index)
            .filter(|s| s.generation == id.generation)
            .and_then(|s| s.node.as_mut())
    }

    fn remove(&mut self, id: NodeId) -> Option<Node> {
        let slot = self.slots.get_mut(id.index)?;
        if slot.generation != id.generation {
            return None;
        }
        let node = slot.node.take();
        if node.is_some() {
            slot.generation = slot.generation.wrapping_add(1);
            self.free.push(id.index);
        }
        node
    }

    fn contains(&self, id: NodeId) -> bool {
        self.get(id).is_some()
    }
}

#[cfg(test)]
mod arena_tests {
    use super::*;

    fn stub() -> Node {
        Node {
            value: None,
            state: State::Clean,
            deps: None,
            deps_tail: None,
            subs: None,
            subs_tail: None,
            owner: None,
            owned: Vec::new(),
            cleanups: Vec::new(),
            contexts: HashMap::new(),
            eq: None,
            kind: Kind::Signal,
        }
    }

    #[test]
    fn reuses_freed_slots_with_a_new_generation() {
        let mut arena = Arena::new();
        let first = arena.insert(stub());
        assert!(arena.remove(first).is_some());

        let second = arena.insert(stub());
        assert_eq!(first.index, second.index); // slot reused
        assert_ne!(first.generation, second.generation); // bumped generation
        assert!(arena.get(first).is_none()); // stale handle no longer resolves
        assert!(arena.get(second).is_some());
    }

    #[test]
    fn removing_a_stale_handle_is_a_noop() {
        let mut arena = Arena::new();
        let id = arena.insert(stub());
        arena.remove(id);
        assert!(arena.remove(id).is_none());
    }
}

#[cfg(test)]
mod link_tests {
    use super::*;

    fn live_links() -> usize {
        RT.with_borrow(|rt| rt.links.iter().filter(|l| l.is_some()).count())
    }

    #[test]
    fn reruns_reuse_links_instead_of_leaking() {
        let count = signal(0);
        effect(move || {
            let _ = count.get();
        });
        let after_first = live_links();
        assert_eq!(after_first, 1); // one edge: effect -> count

        for i in 1..=20 {
            count.set(i); // each write re-runs the effect, re-tracking `count`
        }
        // The single edge is reused every run, never reallocated or leaked.
        assert_eq!(live_links(), 1);
    }

    #[test]
    fn dropped_dependencies_free_their_links() {
        let switch = signal(true);
        let a = signal(1);
        let b = signal(2);
        effect(move || {
            let _ = if switch.get() { a.get() } else { b.get() };
        });
        // Edges: effect -> switch, effect -> a.
        assert_eq!(live_links(), 2);

        switch.set(false); // now reads switch and b; the edge to `a` is pruned.
        assert_eq!(live_links(), 2); // effect -> switch, effect -> b

        switch.set(true);
        assert_eq!(live_links(), 2); // effect -> switch, effect -> a
    }
}

/// Freshness of a reactive node, ordered `Clean < Check < Dirty`.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum State {
    Clean,
    Check,
    Dirty,
}

enum Kind {
    Signal,
    Computed {
        compute: Option<Box<dyn Fn() -> Box<dyn Any>>>,
    },
    Effect {
        run: Option<Box<dyn FnMut()>>,
    },
    /// An ownership-only node created by [`create_root`]; not reactive.
    Root,
}

/// An index into the link pool. Links are internal and only reached through node
/// list heads/tails and their own neighbor pointers, so no generation is needed.
#[derive(Clone, Copy, PartialEq, Eq)]
struct LinkId(usize);

/// One dependency edge `dep -> sub`. Each link is threaded into two intrusive
/// doubly-linked lists at once: `sub`'s dependency list (via `prev_dep`/`next_dep`,
/// kept in read order) and `dep`'s subscriber list (via `prev_sub`/`next_sub`).
/// This lets re-runs reuse links in place and lets disposal unlink in O(1).
struct Link {
    dep: NodeId,
    sub: NodeId,
    prev_dep: Option<LinkId>,
    next_dep: Option<LinkId>,
    prev_sub: Option<LinkId>,
    next_sub: Option<LinkId>,
}

struct Node {
    value: Option<Box<dyn Any>>,
    state: State,
    deps: Option<LinkId>,      // head of this node's dependency list (its sources)
    deps_tail: Option<LinkId>, // tail of the dependency list; also the cursor while tracking
    subs: Option<LinkId>,      // head of this node's subscriber list (its dependents)
    subs_tail: Option<LinkId>, // tail of the subscriber list
    owner: Option<NodeId>,     // owner active when this node was created
    owned: Vec<NodeId>,        // nodes created while this node was the owner
    cleanups: Vec<Box<dyn FnOnce()>>,
    contexts: HashMap<TypeId, Box<dyn Any>>, // values provided to descendants
    eq: Option<EqFn>,                        // optional equality check for dedup
    kind: Kind,
}

/// Compares two stored values for equality, to skip no-op updates.
type EqFn = Box<dyn Fn(&dyn Any, &dyn Any) -> bool>;

struct Runtime {
    nodes: Arena,
    links: Vec<Option<Link>>, // link pool; freed slots are reused via `free_links`
    free_links: Vec<usize>,
    observer: Option<NodeId>, // current dependency-tracking node
    owner: Option<NodeId>,    // current ownership node
    batch_depth: usize,       // >0 while inside batch(): defer effect runs
    pending: VecDeque<NodeId>, // effects deferred by a batch, deduped, FIFO
    flushing: bool,           // true while draining `pending`
    reading_depth: usize,     // >0 while inside a read closure: defer effect runs
    post_flush: Vec<Box<dyn FnOnce()>>, // next_tick callbacks, run after effects
    scheduler: Option<Rc<dyn Fn()>>, // when set, flushes run asynchronously via this hook
    flush_scheduled: bool,    // true once a flush is requested but not yet drained
}

thread_local! {
    static RT: RefCell<Runtime> = RefCell::new(Runtime {
        nodes: Arena::new(),
        links: Vec::new(),
        free_links: Vec::new(),
        observer: None,
        owner: None,
        batch_depth: 0,
        pending: VecDeque::new(),
        flushing: false,
        reading_depth: 0,
        post_flush: Vec::new(),
        scheduler: None,
        flush_scheduled: false,
    });

    /// Callbacks registered via [`on_mounted`], run by [`flush_mounted`].
    static MOUNTED: RefCell<Vec<Box<dyn FnOnce()>>> = const { RefCell::new(Vec::new()) };
}

impl Runtime {
    fn alloc_link(&mut self, link: Link) -> LinkId {
        if let Some(i) = self.free_links.pop() {
            self.links[i] = Some(link);
            LinkId(i)
        } else {
            self.links.push(Some(link));
            LinkId(self.links.len() - 1)
        }
    }

    fn link_ref(&self, id: LinkId) -> &Link {
        self.links[id.0].as_ref().expect("dangling link")
    }

    fn link_mut(&mut self, id: LinkId) -> &mut Link {
        self.links[id.0].as_mut().expect("dangling link")
    }

    fn free_link(&mut self, id: LinkId) {
        self.links[id.0] = None;
        self.free_links.push(id.0);
    }

    /// Record that `sub` depends on `dep`, reusing an existing link when the
    /// dependencies are read in the same order as the previous run (the common
    /// case: O(1), no allocation). New or reordered dependencies are spliced in;
    /// links left after the tracking cursor are pruned by [`Runtime::end_tracking`].
    fn link(&mut self, dep: NodeId, sub: NodeId) {
        let prev_dep = self.nodes.get(sub).and_then(|n| n.deps_tail);
        // Same dependency read twice in a row: already at the cursor.
        if let Some(pd) = prev_dep
            && self.link_ref(pd).dep == dep
        {
            return;
        }
        // Positional reuse: the next link after the cursor already points at `dep`.
        let next_dep = match prev_dep {
            Some(pd) => self.link_ref(pd).next_dep,
            None => self.nodes.get(sub).and_then(|n| n.deps),
        };
        if let Some(nd) = next_dep
            && self.link_ref(nd).dep == dep
        {
            self.nodes.get_mut(sub).unwrap().deps_tail = Some(nd);
            return;
        }
        // Non-consecutive duplicate read already confirmed this run: skip.
        let prev_sub = self.nodes.get(dep).and_then(|n| n.subs_tail);
        if let Some(ps) = prev_sub
            && self.link_ref(ps).sub == sub
            && self.is_valid_link(ps, sub)
        {
            return;
        }
        // A genuinely new edge: allocate a link and splice it into both lists.
        let new = self.alloc_link(Link {
            dep,
            sub,
            prev_dep,
            next_dep,
            prev_sub,
            next_sub: None,
        });
        if let Some(nd) = next_dep {
            self.link_mut(nd).prev_dep = Some(new);
        }
        match prev_dep {
            Some(pd) => self.link_mut(pd).next_dep = Some(new),
            None => self.nodes.get_mut(sub).unwrap().deps = Some(new),
        }
        self.nodes.get_mut(sub).unwrap().deps_tail = Some(new);
        match prev_sub {
            Some(ps) => self.link_mut(ps).next_sub = Some(new),
            None => self.nodes.get_mut(dep).unwrap().subs = Some(new),
        }
        self.nodes.get_mut(dep).unwrap().subs_tail = Some(new);
    }

    /// Is `check` part of `sub`'s already-confirmed dependency prefix this run
    /// (the links from the head up to and including the cursor `deps_tail`)?
    fn is_valid_link(&self, check: LinkId, sub: NodeId) -> bool {
        let Some(tail) = self.nodes.get(sub).and_then(|n| n.deps_tail) else {
            return false;
        };
        let mut cur = self.nodes.get(sub).and_then(|n| n.deps);
        while let Some(c) = cur {
            if c == check {
                return true;
            }
            if c == tail {
                break;
            }
            cur = self.link_ref(c).next_dep;
        }
        false
    }

    /// Reset `sub`'s tracking cursor so the next run reuses links from the head.
    fn start_tracking(&mut self, sub: NodeId) {
        if let Some(n) = self.nodes.get_mut(sub) {
            n.deps_tail = None;
        }
    }

    /// Drop every dependency link left after the cursor: these were read on the
    /// previous run but not this one, so the dependencies are no longer observed.
    fn end_tracking(&mut self, sub: NodeId) {
        let tail = self.nodes.get(sub).and_then(|n| n.deps_tail);
        let mut cur = match tail {
            Some(t) => self.link_ref(t).next_dep,
            None => self.nodes.get(sub).and_then(|n| n.deps),
        };
        match tail {
            Some(t) => self.link_mut(t).next_dep = None,
            None => {
                if let Some(n) = self.nodes.get_mut(sub) {
                    n.deps = None;
                }
            }
        }
        while let Some(c) = cur {
            let next = self.link_ref(c).next_dep;
            self.unlink_from_dep_subs(c);
            self.free_link(c);
            cur = next;
        }
    }

    /// Remove a link from its `dep`'s subscriber list (the `prev_sub`/`next_sub`
    /// chain), repairing the neighbours and the list head/tail.
    fn unlink_from_dep_subs(&mut self, id: LinkId) {
        let (dep, prev, next) = {
            let l = self.link_ref(id);
            (l.dep, l.prev_sub, l.next_sub)
        };
        match prev {
            Some(p) => self.link_mut(p).next_sub = next,
            None => {
                if let Some(n) = self.nodes.get_mut(dep) {
                    n.subs = next;
                }
            }
        }
        match next {
            Some(nx) => self.link_mut(nx).prev_sub = prev,
            None => {
                if let Some(n) = self.nodes.get_mut(dep) {
                    n.subs_tail = prev;
                }
            }
        }
    }

    /// Remove a link from its `sub`'s dependency list (the `prev_dep`/`next_dep`
    /// chain), repairing the neighbours and the list head/tail.
    fn unlink_from_sub_deps(&mut self, id: LinkId) {
        let (sub, prev, next) = {
            let l = self.link_ref(id);
            (l.sub, l.prev_dep, l.next_dep)
        };
        match prev {
            Some(p) => self.link_mut(p).next_dep = next,
            None => {
                if let Some(n) = self.nodes.get_mut(sub) {
                    n.deps = next;
                }
            }
        }
        match next {
            Some(nx) => self.link_mut(nx).prev_dep = prev,
            None => {
                if let Some(n) = self.nodes.get_mut(sub) {
                    n.deps_tail = prev;
                }
            }
        }
    }

    /// Detach `sub` from all of its dependencies (unlinking from each dep's
    /// subscriber list) and free the links. Used when disposing `sub`.
    fn clear_deps(&mut self, sub: NodeId) {
        let mut cur = self.nodes.get(sub).and_then(|n| n.deps);
        while let Some(c) = cur {
            let next = self.link_ref(c).next_dep;
            self.unlink_from_dep_subs(c);
            self.free_link(c);
            cur = next;
        }
        if let Some(n) = self.nodes.get_mut(sub) {
            n.deps = None;
            n.deps_tail = None;
        }
    }

    /// Detach all subscribers from `dep` (unlinking from each subscriber's
    /// dependency list) and free the links. Used when disposing `dep`.
    fn detach_subs(&mut self, dep: NodeId) {
        let mut cur = self.nodes.get(dep).and_then(|n| n.subs);
        while let Some(c) = cur {
            let next = self.link_ref(c).next_sub;
            self.unlink_from_sub_deps(c);
            self.free_link(c);
            cur = next;
        }
        if let Some(n) = self.nodes.get_mut(dep) {
            n.subs = None;
            n.subs_tail = None;
        }
    }

    /// Collect the dependency nodes (sources) of `id` in read order.
    fn collect_deps(&self, id: NodeId) -> Vec<NodeId> {
        let mut out = Vec::new();
        let mut cur = self.nodes.get(id).and_then(|n| n.deps);
        while let Some(c) = cur {
            let l = self.link_ref(c);
            out.push(l.dep);
            cur = l.next_dep;
        }
        out
    }

    /// Collect the subscriber nodes (dependents) of `id`.
    fn collect_subs(&self, id: NodeId) -> Vec<NodeId> {
        let mut out = Vec::new();
        let mut cur = self.nodes.get(id).and_then(|n| n.subs);
        while let Some(c) = cur {
            let l = self.link_ref(c);
            out.push(l.sub);
            cur = l.next_sub;
        }
        out
    }
}

fn new_node(kind: Kind, value: Option<Box<dyn Any>>) -> NodeId {
    RT.with_borrow_mut(|rt| {
        let owner = rt.owner;
        let id = rt.nodes.insert(Node {
            value,
            state: State::Clean,
            deps: None,
            deps_tail: None,
            subs: None,
            subs_tail: None,
            owner,
            owned: Vec::new(),
            cleanups: Vec::new(),
            contexts: HashMap::new(),
            eq: None,
            kind,
        });
        if let Some(owner) = owner
            && let Some(node) = rt.nodes.get_mut(owner)
        {
            node.owned.push(id);
        }
        id
    })
}

/// Record that the currently running observer depends on `source`.
fn record_dependency(source: NodeId) {
    RT.with_borrow_mut(|rt| {
        if let Some(obs) = rt.observer {
            rt.link(source, obs);
        }
    });
}

/// Dispose owned children and run pending cleanups, keeping `id` alive so it can
/// re-run. Does NOT touch dependency edges: re-tracking reuses them in place
/// (see [`Runtime::start_tracking`] / [`Runtime::end_tracking`]).
fn run_owner_teardown(id: NodeId) {
    let owned = RT.with_borrow_mut(|rt| {
        rt.nodes
            .get_mut(id)
            .map(|n| std::mem::take(&mut n.owned))
            .unwrap_or_default()
    });
    for child in owned {
        dispose_node(child);
    }
    let cleanups = RT.with_borrow_mut(|rt| {
        rt.nodes
            .get_mut(id)
            .map(|n| std::mem::take(&mut n.cleanups))
            .unwrap_or_default()
    });
    for cleanup in cleanups.into_iter().rev() {
        cleanup();
    }
}

/// Fully remove a node: dispose its children, run its cleanups, detach all edges.
fn dispose_node(id: NodeId) {
    if RT.with_borrow(|rt| !rt.nodes.contains(id)) {
        return;
    }
    run_owner_teardown(id); // dispose children, run cleanups
    RT.with_borrow_mut(|rt| {
        rt.clear_deps(id); // unlink from each dependency's subscriber list
        rt.detach_subs(id); // unlink from each subscriber's dependency list
        // Detach from owner's owned list.
        if let Some(owner) = rt.nodes.get(id).and_then(|n| n.owner)
            && let Some(node) = rt.nodes.get_mut(owner)
        {
            node.owned.retain(|x| *x != id);
        }
        rt.nodes.remove(id);
    });
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

fn node_state(id: NodeId) -> Option<State> {
    RT.with_borrow(|rt| rt.nodes.get(id).map(|n| n.state))
}

fn set_state(id: NodeId, state: State) {
    RT.with_borrow_mut(|rt| {
        if let Some(node) = rt.nodes.get_mut(id) {
            node.state = state;
        }
    });
}

/// Escalate `id` to `target` and mark its transitive subscribers `Check`,
/// queuing any effect that leaves the `Clean` state.
fn mark_stale(id: NodeId, target: State) {
    let subs = RT.with_borrow_mut(|rt| {
        let (was_clean, is_effect) = {
            let node = rt.nodes.get_mut(id)?;
            if node.state >= target {
                return None;
            }
            let was_clean = node.state == State::Clean;
            node.state = target;
            (was_clean, matches!(node.kind, Kind::Effect { .. }))
        };
        if was_clean && is_effect && !rt.pending.contains(&id) {
            rt.pending.push_back(id);
        }
        Some(rt.collect_subs(id))
    });
    if let Some(subs) = subs {
        for s in subs {
            mark_stale(s, State::Check);
        }
    }
}

/// Notify `id`'s direct subscribers that it changed, then flush if able.
fn notify_subscribers(id: NodeId) {
    let subs = RT.with_borrow(|rt| rt.collect_subs(id));
    for s in subs {
        mark_stale(s, State::Dirty);
    }
    maybe_flush();
}

/// Bring `id` up to date if any source may have changed (pull-based evaluation).
fn update_if_necessary(id: NodeId) {
    if node_state(id) == Some(State::Check) {
        let sources = RT.with_borrow(|rt| rt.collect_deps(id));
        for source in sources {
            update_if_necessary(source);
            if node_state(id) == Some(State::Dirty) {
                break;
            }
        }
    }
    if node_state(id) == Some(State::Dirty) {
        update(id);
    }
    set_state(id, State::Clean);
}

/// Recompute a computed (dirtying observers only if its value changed), or run an
/// effect. Disposes owned children and runs cleanups first; dependency links are
/// re-tracked in place (reused when unchanged, pruned when no longer read).
fn update(id: NodeId) {
    let is_effect = RT.with_borrow(|rt| {
        matches!(rt.nodes.get(id).map(|n| &n.kind), Some(Kind::Effect { .. }))
    });

    if is_effect {
        let run = RT.with_borrow_mut(|rt| match rt.nodes.get_mut(id).map(|n| &mut n.kind) {
            Some(Kind::Effect { run }) => run.take(),
            _ => None,
        });
        let Some(mut run) = run else { return };
        run_owner_teardown(id);
        RT.with_borrow_mut(|rt| rt.start_tracking(id));
        #[allow(clippy::redundant_closure)]
        run_scoped(id, || run());
        RT.with_borrow_mut(|rt| {
            rt.end_tracking(id);
            if let Some(Kind::Effect { run: slot }) = rt.nodes.get_mut(id).map(|n| &mut n.kind) {
                *slot = Some(run);
            }
        });
        return;
    }

    // Computed.
    let compute = RT.with_borrow_mut(|rt| match rt.nodes.get_mut(id).map(|n| &mut n.kind) {
        Some(Kind::Computed { compute }) => compute.take(),
        _ => None,
    });
    let Some(compute) = compute else { return };
    run_owner_teardown(id);
    RT.with_borrow_mut(|rt| rt.start_tracking(id));
    let mut new_value: Option<Box<dyn Any>> = None;
    run_scoped(id, || new_value = Some(compute()));
    RT.with_borrow_mut(|rt| rt.end_tracking(id));
    let new_value = new_value.expect("compute produced no value");

    let changed = RT.with_borrow_mut(|rt| {
        let Some(node) = rt.nodes.get_mut(id) else {
            return false;
        };
        if let Kind::Computed { compute: slot } = &mut node.kind {
            *slot = Some(compute);
        }
        let changed = match (&node.eq, &node.value) {
            (Some(eq), Some(old)) => !eq(&**old, &*new_value),
            _ => true,
        };
        node.value = Some(new_value);
        changed
    });

    if changed {
        // The value changed, so observers (already `Check`) must recompute.
        let subs = RT.with_borrow(|rt| rt.collect_subs(id));
        RT.with_borrow_mut(|rt| {
            for s in subs {
                if let Some(node) = rt.nodes.get_mut(s)
                    && node.state < State::Dirty
                {
                    node.state = State::Dirty;
                }
            }
        });
    }
}

fn flush_pending() {
    RT.with_borrow_mut(|rt| rt.flushing = true);
    loop {
        // Drain all queued effects.
        loop {
            let next = RT.with_borrow_mut(|rt| rt.pending.pop_front());
            match next {
                Some(e) => update_if_necessary(e),
                None => break,
            }
        }
        // Then run next_tick callbacks; these may enqueue more work, so loop.
        let callbacks = RT.with_borrow_mut(|rt| std::mem::take(&mut rt.post_flush));
        if callbacks.is_empty() {
            break;
        }
        for callback in callbacks {
            callback();
        }
    }
    RT.with_borrow_mut(|rt| rt.flushing = false);
}

fn read_with<T: 'static, R>(id: NodeId, f: impl FnOnce(&T) -> R) -> R {
    record_dependency(id);
    update_if_necessary(id);
    // Take the value out and run `f` WITHOUT holding the runtime borrow, so `f`
    // may read or write other reactive values. Writes during a read are deferred
    // and flushed once the read completes.
    let value = RT.with_borrow_mut(|rt| {
        rt.reading_depth += 1;
        rt.nodes
            .get_mut(id)
            .and_then(|n| n.value.take())
            .expect("node has no value")
    });
    let result = {
        let typed = value
            .downcast_ref::<T>()
            .expect("type mismatch reading reactive node");
        f(typed)
    };
    RT.with_borrow_mut(|rt| {
        if let Some(node) = rt.nodes.get_mut(id) {
            node.value = Some(value);
        }
        rt.reading_depth -= 1;
    });
    maybe_flush();
    result
}

/// Flush deferred effects and next_tick callbacks once no read or batch is active.
/// With an async scheduler installed the flush is deferred to a host microtask
/// instead of running inline.
fn maybe_flush() {
    let ready = RT.with_borrow(|rt| {
        rt.reading_depth == 0
            && rt.batch_depth == 0
            && !rt.flushing
            && (!rt.pending.is_empty() || !rt.post_flush.is_empty())
    });
    if ready {
        schedule_or_flush();
    }
}

/// Drain queued work now if flushing is synchronous, or ask the installed
/// scheduler to drain it later (deduped: at most one pending request).
fn schedule_or_flush() {
    let hook = RT.with_borrow_mut(|rt| match &rt.scheduler {
        Some(_) if rt.flush_scheduled => None,
        Some(hook) => {
            rt.flush_scheduled = true;
            Some(hook.clone())
        }
        None => None,
    });
    match hook {
        Some(hook) => hook(),
        None if RT.with_borrow(|rt| rt.scheduler.is_none()) => flush_pending(),
        None => {} // a flush is already scheduled
    }
}

fn set_value<T: 'static>(id: NodeId, v: T) {
    let changed = RT.with_borrow_mut(|rt| {
        let Some(node) = rt.nodes.get_mut(id) else {
            return false;
        };
        let changed = match (&node.eq, &node.value) {
            (Some(eq), Some(old)) => !eq(&**old, &v),
            _ => true,
        };
        node.value = Some(Box::new(v));
        changed
    });
    if changed {
        notify_subscribers(id);
    }
}

fn update_value<T: 'static>(id: NodeId, f: impl FnOnce(&mut T)) {
    RT.with_borrow_mut(|rt| {
        let any = rt
            .nodes
            .get_mut(id)
            .and_then(|n| n.value.as_mut())
            .expect("node has no value");
        let v = any
            .downcast_mut::<T>()
            .expect("type mismatch updating reactive node");
        f(v);
    });
    notify_subscribers(id);
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

// Handles compare by identity (which node they point at).
impl<T> PartialEq for Signal<T> {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}
impl<T> Eq for Signal<T> {}

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

impl<T> PartialEq for Memo<T> {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}
impl<T> Eq for Memo<T> {}

/// Create a writable reactive value. Setting it to a value equal to the current
/// one does not notify dependents (Vue `ref`-style change check).
pub fn signal<T: PartialEq + 'static>(value: T) -> Signal<T> {
    let id = new_node(Kind::Signal, Some(Box::new(value)));
    install_eq::<T>(id);
    Signal {
        id,
        _t: PhantomData,
    }
}

/// Like [`signal`], but without an equality check: accepts any value (including
/// non-`PartialEq` types) and notifies dependents on every set.
pub fn signal_raw<T: 'static>(value: T) -> Signal<T> {
    Signal {
        id: new_node(Kind::Signal, Some(Box::new(value))),
        _t: PhantomData,
    }
}

/// Install an equality check on a node so updates to an equal value are skipped.
fn install_eq<T: PartialEq + 'static>(id: NodeId) {
    RT.with_borrow_mut(|rt| {
        if let Some(node) = rt.nodes.get_mut(id) {
            node.eq = Some(Box::new(|a: &dyn Any, b: &dyn Any| {
                matches!(
                    (a.downcast_ref::<T>(), b.downcast_ref::<T>()),
                    (Some(x), Some(y)) if x == y
                )
            }));
        }
    });
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

/// Create a memoized derived value. When it recomputes to a value equal to the
/// previous one, its dependents are not re-run (Vue `computed`-style dedup).
pub fn computed<T: PartialEq + 'static>(f: impl Fn() -> T + 'static) -> Memo<T> {
    let id = new_computed(f);
    install_eq::<T>(id);
    Memo {
        id,
        _t: PhantomData,
    }
}

/// Like [`computed`], but without an equality check: accepts any value and always
/// dirties dependents when a dependency changes.
pub fn computed_raw<T: 'static>(f: impl Fn() -> T + 'static) -> Memo<T> {
    Memo {
        id: new_computed(f),
        _t: PhantomData,
    }
}

fn new_computed<T: 'static>(f: impl Fn() -> T + 'static) -> NodeId {
    let compute: Box<dyn Fn() -> Box<dyn Any>> = Box::new(move || Box::new(f()) as Box<dyn Any>);
    let id = new_node(Kind::Computed { compute: Some(compute) }, None);
    set_state(id, State::Dirty); // not yet evaluated
    id
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

/// Schedule `callback` to run after pending effects have flushed (Vue's
/// `nextTick`). With synchronous flushing it runs immediately when nothing is in
/// flight; with an async scheduler installed it always runs on the next flush.
pub fn next_tick(callback: impl FnOnce() + 'static) {
    let (has_scheduler, defer) = RT.with_borrow(|rt| {
        let defer = rt.batch_depth > 0
            || rt.flushing
            || rt.reading_depth > 0
            || !rt.pending.is_empty();
        (rt.scheduler.is_some(), defer)
    });
    if has_scheduler {
        RT.with_borrow_mut(|rt| rt.post_flush.push(Box::new(callback)));
        schedule_or_flush();
    } else if defer {
        RT.with_borrow_mut(|rt| rt.post_flush.push(Box::new(callback)));
    } else {
        callback();
    }
}

/// Install an async flush scheduler. While set, effect re-runs and
/// [`next_tick`] callbacks are coalesced and deferred: `hook` is invoked (at
/// most once per pending flush) to ask the host to call [`flush_jobs`] later,
/// e.g. from a `queueMicrotask` callback in the browser.
pub fn set_scheduler(hook: impl Fn() + 'static) {
    RT.with_borrow_mut(|rt| rt.scheduler = Some(Rc::new(hook)));
}

/// Remove the async scheduler, restoring synchronous flushing. Any work already
/// queued is left for the next synchronous flush.
pub fn clear_scheduler() {
    RT.with_borrow_mut(|rt| {
        rt.scheduler = None;
        rt.flush_scheduled = false;
    });
}

/// Drain queued effects and [`next_tick`] callbacks. The host calls this from
/// the microtask scheduled by [`set_scheduler`]'s hook. A no-op when nothing is
/// queued.
pub fn flush_jobs() {
    RT.with_borrow_mut(|rt| rt.flush_scheduled = false);
    flush_pending();
}

/// Run `f`, deferring all effect re-runs until it returns, so multiple writes
/// trigger each affected effect at most once. Batches may nest.
pub fn batch<T>(f: impl FnOnce() -> T) -> T {
    RT.with_borrow_mut(|rt| rt.batch_depth += 1);
    let result = f();
    let depth = RT.with_borrow_mut(|rt| {
        rt.batch_depth -= 1;
        rt.batch_depth
    });
    if depth == 0 {
        schedule_or_flush();
    }
    result
}

/// Run `f` immediately and re-run it whenever any reactive value it read changes.
pub fn effect(f: impl FnMut() + 'static) {
    let id = new_node(
        Kind::Effect {
            run: Some(Box::new(f)),
        },
        None,
    );
    set_state(id, State::Dirty);
    update_if_necessary(id); // initial run
}

/// Register a cleanup to run before the current owner re-runs, and when it is
/// disposed. No-op outside a reactive scope.
pub fn on_cleanup(f: impl FnOnce() + 'static) {
    RT.with_borrow_mut(|rt| {
        if let Some(owner) = rt.owner
            && let Some(node) = rt.nodes.get_mut(owner) {
                node.cleanups.push(Box::new(f));
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
/// The root is owned by the surrounding scope (disposed with it).
pub fn create_root(f: impl FnOnce()) -> RootDisposer {
    run_in_new_root(false, f)
}

/// Like [`create_root`], but the root is not owned by the surrounding scope, so
/// re-running an enclosing effect does not dispose it. The caller must dispose it
/// explicitly. Used by control flow that manages branch lifetimes by hand.
pub fn create_root_detached(f: impl FnOnce()) -> RootDisposer {
    run_in_new_root(true, f)
}

/// Run `f` inside a child ownership scope owned by the surrounding scope. Used by
/// components so their effects and provided contexts are scoped to the subtree
/// (disposed with the parent). Returns `f`'s result.
pub fn run_in_child_scope<T>(f: impl FnOnce() -> T) -> T {
    let id = new_node(Kind::Root, None); // owned by the current owner
    let (prev_obs, prev_owner) = RT.with_borrow_mut(|rt| {
        let p = (rt.observer, rt.owner);
        rt.observer = None; // a scope body itself is not reactive
        rt.owner = Some(id);
        p
    });
    let result = f();
    RT.with_borrow_mut(|rt| {
        rt.observer = prev_obs;
        rt.owner = prev_owner;
    });
    result
}

/// Register a callback to run when the current scope is disposed (unmounted).
/// For a component scope this fires once, when the component is removed.
pub fn on_unmounted(callback: impl FnOnce() + 'static) {
    on_cleanup(callback);
}

/// Register a callback to run after the tree is mounted (see [`flush_mounted`]).
pub fn on_mounted(callback: impl FnOnce() + 'static) {
    MOUNTED.with_borrow_mut(|queue| queue.push(Box::new(callback)));
}

/// Run and clear all callbacks registered via [`on_mounted`]. Call this once the
/// rendered tree has been inserted into the document.
pub fn flush_mounted() {
    let callbacks = MOUNTED.with_borrow_mut(std::mem::take);
    for callback in callbacks {
        callback();
    }
}

/// Provide a value to descendant scopes, keyed by its type. Overrides any value
/// of the same type provided by an ancestor. No-op outside a scope.
pub fn provide_context<T: 'static>(value: T) {
    RT.with_borrow_mut(|rt| {
        if let Some(owner) = rt.owner
            && let Some(node) = rt.nodes.get_mut(owner)
        {
            node.contexts.insert(TypeId::of::<T>(), Box::new(value));
        }
    });
}

/// Look up the nearest value of type `T` provided by an ancestor scope.
pub fn use_context<T: Clone + 'static>() -> Option<T> {
    RT.with_borrow(|rt| {
        let mut cur = rt.owner;
        while let Some(id) = cur {
            let node = rt.nodes.get(id)?;
            if let Some(value) = node.contexts.get(&TypeId::of::<T>()) {
                return value.downcast_ref::<T>().cloned();
            }
            cur = node.owner;
        }
        None
    })
}

fn run_in_new_root(detached: bool, f: impl FnOnce()) -> RootDisposer {
    let id = if detached {
        // Create the root with no owner so it survives enclosing effect re-runs.
        let prev = RT.with_borrow_mut(|rt| rt.owner.take());
        let id = new_node(Kind::Root, None);
        RT.with_borrow_mut(|rt| rt.owner = prev);
        id
    } else {
        new_node(Kind::Root, None)
    };
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
