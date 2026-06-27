use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::hash::Hash;
use std::rc::Rc;

use vue_rs_reactive::{create_root_detached, effect, RootDisposer};

use crate::backend::Backend;

/// A mounted dynamic branch: its root node plus the disposer for its effects.
type Branch<B> = (<B as Backend>::Node, RootDisposer);

/// Builder for an element node. Reactive bindings (`dyn_text` / `dyn_attr`) and
/// control flow (`dyn_if` / `dyn_for`) install effects so the tree updates when
/// the reactive values they read change.
pub struct El<B: Backend> {
    backend: B,
    node: B::Node,
}

impl<B: Backend> El<B> {
    /// Create a new element with the given tag.
    pub fn new(backend: B, tag: &str) -> Self {
        let node = backend.create_element(tag);
        Self { backend, node }
    }

    /// Set a static attribute.
    pub fn attr(self, name: &str, value: &str) -> Self {
        self.backend.set_attribute(&self.node, name, value);
        self
    }

    /// Set an attribute that re-evaluates whenever its reactive deps change.
    pub fn dyn_attr(self, name: &str, f: impl Fn() -> String + 'static) -> Self {
        let backend = self.backend.clone();
        let node = self.node.clone();
        let name = name.to_string();
        effect(move || backend.set_attribute(&node, &name, &f()));
        self
    }

    /// Append a static text child.
    pub fn text(self, data: &str) -> Self {
        let text = self.backend.create_text(data);
        self.backend.append_child(&self.node, &text);
        self
    }

    /// Append a text child that re-evaluates whenever its reactive deps change.
    pub fn dyn_text(self, f: impl Fn() -> String + 'static) -> Self {
        let text = self.backend.create_text("");
        self.backend.append_child(&self.node, &text);
        let backend = self.backend.clone();
        let text_node = text;
        effect(move || backend.set_text(&text_node, &f()));
        self
    }

    /// Append an already-built child node.
    pub fn child(self, child: B::Node) -> Self {
        self.backend.append_child(&self.node, &child);
        self
    }

    /// Attach an event listener that ignores the event value.
    pub fn on(self, event: &str, handler: impl Fn() + 'static) -> Self {
        self.backend
            .add_event_listener(&self.node, event, Rc::new(move |_value: &str| handler()));
        self
    }

    /// Attach an event listener that receives the event value (e.g. input text).
    pub fn on_value(self, event: &str, handler: impl Fn(&str) + 'static) -> Self {
        self.backend
            .add_event_listener(&self.node, event, Rc::new(move |value: &str| handler(value)));
        self
    }

    /// Conditionally mount a child: build it when `cond` becomes true, remove it
    /// when it becomes false.
    pub fn dyn_if<F, V>(self, cond: F, view: V) -> Self
    where
        F: Fn() -> bool + 'static,
        V: Fn(B) -> B::Node + 'static,
    {
        let anchor = self.backend.create_anchor();
        self.backend.append_child(&self.node, &anchor);
        let backend = self.backend.clone();
        let parent = self.node.clone();
        let mounted: Rc<RefCell<Option<Branch<B>>>> = Rc::new(RefCell::new(None));
        let shown = Cell::new(false);
        effect(move || {
            let show = cond();
            if show == shown.get() {
                return;
            }
            shown.set(show);
            if show {
                let (node, disposer) = build_branch(&backend, &view);
                backend.insert_before(&parent, &node, &anchor);
                *mounted.borrow_mut() = Some((node, disposer));
            } else if let Some((node, disposer)) = mounted.borrow_mut().take() {
                backend.remove_child(&parent, &node);
                disposer.dispose();
            }
        });
        self
    }

    /// Mount one of two branches depending on `cond` (the `v-if` / `v-else` pair).
    pub fn dyn_if_else<F, V1, V2>(self, cond: F, then_view: V1, else_view: V2) -> Self
    where
        F: Fn() -> bool + 'static,
        V1: Fn(B) -> B::Node + 'static,
        V2: Fn(B) -> B::Node + 'static,
    {
        let anchor = self.backend.create_anchor();
        self.backend.append_child(&self.node, &anchor);
        let backend = self.backend.clone();
        let parent = self.node.clone();
        let mounted: Rc<RefCell<Option<Branch<B>>>> = Rc::new(RefCell::new(None));
        let current = Cell::new(None::<bool>);
        effect(move || {
            let show = cond();
            if current.get() == Some(show) {
                return;
            }
            current.set(Some(show));
            if let Some((node, disposer)) = mounted.borrow_mut().take() {
                backend.remove_child(&parent, &node);
                disposer.dispose();
            }
            let (node, disposer) = if show {
                build_branch(&backend, &then_view)
            } else {
                build_branch(&backend, &else_view)
            };
            backend.insert_before(&parent, &node, &anchor);
            *mounted.borrow_mut() = Some((node, disposer));
        });
        self
    }

    /// Render a keyed list. Rows are reused across updates by their key; rows are
    /// created, removed, and reordered to match `items`.
    pub fn dyn_for<T, K, IT, KF, V>(self, items: IT, key: KF, view: V) -> Self
    where
        T: Clone + 'static,
        K: Eq + Hash + Clone + 'static,
        IT: Fn() -> Vec<T> + 'static,
        KF: Fn(&T) -> K + 'static,
        V: Fn(B, T) -> B::Node + 'static,
    {
        let anchor = self.backend.create_anchor();
        self.backend.append_child(&self.node, &anchor);
        let backend = self.backend.clone();
        let parent = self.node.clone();
        let rows: Rc<RefCell<HashMap<K, Branch<B>>>> =
            Rc::new(RefCell::new(HashMap::new()));
        effect(move || {
            let next = items();
            let mut old = rows.borrow_mut();
            let mut result: HashMap<K, Branch<B>> = HashMap::new();
            let mut ordered: Vec<B::Node> = Vec::with_capacity(next.len());
            for item in next {
                let k = key(&item);
                let entry = old.remove(&k).unwrap_or_else(|| {
                    let mut built = None;
                    let disposer = create_root_detached(|| {
                        built = Some(view(backend.clone(), item.clone()));
                    });
                    (built.expect("view did not build a node"), disposer)
                });
                ordered.push(entry.0.clone());
                result.insert(k, entry);
            }
            // Anything left in `old` was removed from the list.
            for (_key, (node, disposer)) in old.drain() {
                backend.remove_child(&parent, &node);
                disposer.dispose();
            }
            // Reinsert in list order (insert_before moves existing nodes).
            for node in &ordered {
                backend.insert_before(&parent, node, &anchor);
            }
            *old = result;
        });
        self
    }

    /// Finish building and return the node.
    pub fn finish(self) -> B::Node {
        self.node
    }
}

/// Build a view inside a detached reactive scope so it survives re-runs of the
/// control-flow effect; the returned disposer tears down its effects on removal.
fn build_branch<B: Backend>(backend: &B, view: &dyn Fn(B) -> B::Node) -> (B::Node, RootDisposer) {
    let mut built = None;
    let disposer = create_root_detached(|| {
        built = Some(view(backend.clone()));
    });
    (built.expect("view did not build a node"), disposer)
}
