use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::hash::Hash;
use std::rc::Rc;

use vue_rs_reactive::{batch, create_root_detached, effect, on_cleanup, RootDisposer};

use crate::backend::{Backend, EventOptions};
use crate::node_ref::TemplateRef;

/// A mounted dynamic branch: its root node plus the disposer for its effects.
type Branch<B> = (<B as Backend>::Node, RootDisposer);

/// The live `(event name, listener)` of an `on_named` binding, shared between its
/// resubscribing effect and the cleanup that detaches the final listener.
type CurrentListener<B> = Rc<RefCell<Option<(String, <B as Backend>::Listener)>>>;

/// Trusted raw HTML markup for the `v-html` directive.
///
/// The wrapped markup is inserted into the DOM **verbatim, with no escaping**.
/// Constructing a `RawHtml` is the explicit assertion that the markup is safe to
/// render. Building one from untrusted input (user text, network responses,
/// URL parameters) is a cross-site-scripting (XSS) vector — sanitize first. The
/// constructor is named loudly for the same reason React spells this
/// `dangerouslySetInnerHTML`.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct RawHtml(String);

impl RawHtml {
    /// Wrap markup you trust to be safe. The content bypasses all escaping; see
    /// the [`RawHtml`] type docs for the XSS caveat.
    pub fn dangerously_from_html(html: impl Into<String>) -> Self {
        RawHtml(html.into())
    }

    /// The wrapped markup.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

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

    /// Set a DOM property that re-evaluates whenever its reactive deps change
    /// (the `:name.prop` binding), e.g. `node.value`.
    pub fn dyn_prop(self, name: &str, f: impl Fn() -> String + 'static) -> Self {
        let backend = self.backend.clone();
        let node = self.node.clone();
        let name = name.to_string();
        effect(move || backend.set_property(&node, &name, &f()));
        self
    }

    /// Set a boolean DOM property that re-evaluates whenever its reactive deps
    /// change, e.g. a checkbox's `checked` (the `v-model` binding on
    /// `<input type="checkbox">`).
    pub fn dyn_bool_prop(self, name: &str, f: impl Fn() -> bool + 'static) -> Self {
        let backend = self.backend.clone();
        let node = self.node.clone();
        let name = name.to_string();
        effect(move || backend.set_bool_property(&node, &name, f()));
        self
    }

    /// Set an attribute whose *name* is also reactive (the `:[key]` dynamic
    /// argument). Both the name and value re-evaluate when their deps change;
    /// when the name changes, the previously-set attribute is removed first so a
    /// stale attribute does not linger.
    pub fn dyn_attr_named(
        self,
        name: impl Fn() -> String + 'static,
        value: impl Fn() -> String + 'static,
    ) -> Self {
        let backend = self.backend.clone();
        let node = self.node.clone();
        let mut prev: Option<String> = None;
        effect(move || {
            let next = name();
            let value = value();
            if let Some(old) = &prev
                && *old != next
            {
                backend.remove_attribute(&node, old);
            }
            backend.set_attribute(&node, &next, &value);
            prev = Some(next);
        });
        self
    }

    /// Spread a reactive bag of attributes onto the element (the `v-bind="obj"`
    /// directive). Each run sets every `(name, value)` pair; a name that was set
    /// on the previous run but is absent now is removed, so a stale attribute
    /// does not linger.
    pub fn dyn_attrs(self, f: impl Fn() -> Vec<(String, String)> + 'static) -> Self {
        let backend = self.backend.clone();
        let node = self.node.clone();
        let mut prev: Vec<String> = Vec::new();
        effect(move || {
            let next = f();
            for name in &prev {
                if !next.iter().any(|(n, _)| n == name) {
                    backend.remove_attribute(&node, name);
                }
            }
            for (name, value) in &next {
                backend.set_attribute(&node, name, value);
            }
            prev = next.into_iter().map(|(name, _)| name).collect();
        });
        self
    }

    /// Set the element's inner HTML from a [`RawHtml`] value that re-evaluates
    /// whenever its reactive deps change (the `v-html` directive). The markup is
    /// inserted unescaped and replaces any children; requiring `RawHtml` keeps
    /// that opt-in visible at the call site.
    pub fn dyn_inner_html(self, f: impl Fn() -> RawHtml + 'static) -> Self {
        let backend = self.backend.clone();
        let node = self.node.clone();
        effect(move || backend.set_inner_html(&node, f().as_str()));
        self
    }

    /// Bind a template ref (the `ref="name"` directive): store this element's
    /// node in `target` so it can be read after mount. The slot is cleared when
    /// the enclosing reactive scope is disposed, so a removed element does not
    /// leave a dangling node behind.
    pub fn node_ref(self, target: &TemplateRef<B>) -> Self {
        target.set(Some(self.node.clone()));
        let slot = target.slot();
        on_cleanup(move || *slot.borrow_mut() = None);
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

    /// Attach an event listener that ignores the event value. Writes made by the
    /// handler are batched, so dependent effects run at most once per event.
    pub fn on(self, event: &str, handler: impl Fn() + 'static) -> Self {
        self.on_opts(event, EventOptions::default(), handler)
    }

    /// Attach a value-ignoring event listener with explicit modifiers (the
    /// `@event.prevent` / `.stop` / `.once` directive). Writes made by the
    /// handler are batched.
    pub fn on_opts(
        self,
        event: &str,
        options: EventOptions,
        handler: impl Fn() + 'static,
    ) -> Self {
        let listener = self.backend.add_event_listener(
            &self.node,
            event,
            options,
            // `handler` is captured by the listener (Fn), so it can't be moved
            // into `batch`; wrap it in a closure.
            #[allow(clippy::redundant_closure)]
            Rc::new(move |_value: &str| batch(|| handler())),
        );
        self.cleanup_listener(listener);
        self
    }

    /// Attach an event listener that receives the event value (e.g. input text).
    /// Writes made by the handler are batched.
    pub fn on_value(self, event: &str, handler: impl Fn(&str) + 'static) -> Self {
        let listener = self.backend.add_event_listener(
            &self.node,
            event,
            EventOptions::default(),
            Rc::new(move |value: &str| batch(|| handler(value))),
        );
        self.cleanup_listener(listener);
        self
    }

    /// Attach a value-ignoring event listener whose *event name* is reactive (the
    /// `@[event]` dynamic argument). When the name changes, the listener is
    /// detached and re-attached under the new name. Writes made by the handler are
    /// batched.
    pub fn on_named(
        self,
        event: impl Fn() -> String + 'static,
        handler: impl Fn() + 'static,
    ) -> Self {
        // `handler` is captured by the listener (Fn), so it can't be moved into
        // `batch`; wrap it in a closure.
        #[allow(clippy::redundant_closure)]
        let handler: Rc<dyn Fn(&str)> = Rc::new(move |_value: &str| batch(|| handler()));
        // The current `(name, listener)` is shared between the resubscribing
        // effect and the cleanup that detaches the final listener on disposal.
        let current: CurrentListener<B> = Rc::new(RefCell::new(None));
        let backend = self.backend.clone();
        let node = self.node.clone();
        let current_effect = current.clone();
        effect(move || {
            let next = event();
            let mut cur = current_effect.borrow_mut();
            if cur.as_ref().is_some_and(|(name, _)| *name == next) {
                return;
            }
            if let Some((_, listener)) = cur.take() {
                backend.remove_event_listener(&node, listener);
            }
            let listener =
                backend.add_event_listener(&node, &next, EventOptions::default(), handler.clone());
            *cur = Some((next, listener));
        });
        let backend = self.backend.clone();
        let node = self.node.clone();
        on_cleanup(move || {
            if let Some((_, listener)) = current.borrow_mut().take() {
                backend.remove_event_listener(&node, listener);
            }
        });
        self
    }

    /// Detach `listener` when the enclosing reactive scope is disposed, so the
    /// listener (and any closure it owns) is released with the node.
    fn cleanup_listener(&self, listener: B::Listener) {
        let backend = self.backend.clone();
        let node = self.node.clone();
        on_cleanup(move || backend.remove_event_listener(&node, listener));
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
        dispose_branches_on_cleanup::<B>(mounted.clone());
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
        dispose_branches_on_cleanup::<B>(mounted.clone());
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

    /// Mount whichever branch `selector` picks, by index into `views`, or nothing
    /// when it returns `None` (the `v-if` / `v-else-if` / `v-else` chain). Swaps
    /// the mounted branch when the selected index changes.
    pub fn dyn_switch<F>(self, selector: F, views: Vec<Box<dyn Fn(B) -> B::Node>>) -> Self
    where
        F: Fn() -> Option<usize> + 'static,
    {
        let anchor = self.backend.create_anchor();
        self.backend.append_child(&self.node, &anchor);
        let backend = self.backend.clone();
        let parent = self.node.clone();
        let mounted: Rc<RefCell<Option<Branch<B>>>> = Rc::new(RefCell::new(None));
        dispose_branches_on_cleanup::<B>(mounted.clone());
        let current = Cell::new(None::<Option<usize>>);
        effect(move || {
            let which = selector();
            if current.get() == Some(which) {
                return;
            }
            current.set(Some(which));
            if let Some((node, disposer)) = mounted.borrow_mut().take() {
                backend.remove_child(&parent, &node);
                disposer.dispose();
            }
            if let Some(i) = which {
                let (node, disposer) = build_branch(&backend, &*views[i]);
                backend.insert_before(&parent, &node, &anchor);
                *mounted.borrow_mut() = Some((node, disposer));
            }
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
        // Rows are built in detached roots so they survive the list effect's
        // re-runs; register a cleanup so the enclosing scope's disposal tears down
        // any rows still mounted at that point.
        let rows_for_cleanup = rows.clone();
        on_cleanup(move || {
            for (_key, (_node, disposer)) in rows_for_cleanup.borrow_mut().drain() {
                disposer.dispose();
            }
        });
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

/// Build an empty `dyn_switch` branch-view vec whose backend type is pinned from
/// `sample`. Passing the in-scope backend lets each pushed view closure infer its
/// parameter type, so generated code needs no parameter annotation.
pub fn switch_views<B: Backend>(_sample: &B) -> Vec<Box<dyn Fn(B) -> B::Node>> {
    Vec::new()
}

/// Build a standalone reactive text node that re-evaluates whenever its deps
/// change. Unlike [`El::dyn_text`] (which appends into a parent element), this
/// returns the text node itself, for use as a fragment member at a template root
/// where there is no enclosing element.
pub fn dyn_text_node<B: Backend>(backend: &B, f: impl Fn() -> String + 'static) -> B::Node {
    let text = backend.create_text("");
    let node = text.clone();
    let backend = backend.clone();
    effect(move || backend.set_text(&node, &f()));
    text
}

/// Register a cleanup so the enclosing reactive scope's disposal tears down a
/// branch that is still mounted. The branch lives in a detached root (so it
/// survives the control-flow effect's re-runs), which means it is otherwise
/// unowned and would leak when the scope around it goes away.
fn dispose_branches_on_cleanup<B: Backend>(mounted: Rc<RefCell<Option<Branch<B>>>>) {
    on_cleanup(move || {
        if let Some((_node, disposer)) = mounted.borrow_mut().take() {
            disposer.dispose();
        }
    });
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
