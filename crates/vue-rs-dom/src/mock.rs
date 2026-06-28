use std::cell::{Cell, RefCell};
use std::fmt::Write as _;
use std::rc::Rc;

use crate::backend::{Backend, EventOptions};

/// An event handler receiving the event's value.
type Handler = Rc<dyn Fn(&str)>;
/// A registered listener: a unique id (for removal), its event name, modifiers,
/// and handler.
type Listener = (usize, String, EventOptions, Handler);

/// What dispatching an event requested via its listeners' modifiers. Returned by
/// [`MockDom::dispatch`] so tests can assert `.prevent` / `.stop` wiring without a
/// real event object.
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub struct DispatchOutcome {
    /// A matched listener (whose guards passed) was registered with `prevent_default`.
    pub default_prevented: bool,
    /// A matched listener (whose guards passed) was registered with `stop_propagation`.
    pub propagation_stopped: bool,
}

/// Simulated event data for [`MockDom::dispatch_event`], standing in for the
/// fields a real DOM event would carry. Used to exercise the guard modifiers
/// (`.self` / key / mouse-button) without a browser.
#[derive(Clone, Default, Debug)]
pub struct MockEvent {
    /// `event.key` (key modifiers). `None` means the event carries no key.
    pub key: Option<String>,
    /// `event.button` (mouse-button modifiers). `None` means no button.
    pub button: Option<u16>,
    /// The node the event originated on (`event.target`). `None` means the event
    /// targeted the dispatched node itself, so `.self` passes.
    pub target: Option<usize>,
}

enum NodeData {
    Element {
        tag: String,
        attrs: Vec<(String, String)>,
        children: Vec<usize>,
        listeners: Vec<Listener>,
        /// Raw markup set via `set_inner_html` (the `v-html` directive). When
        /// present it is serialized verbatim in place of `children`.
        inner_html: Option<String>,
    },
    Text {
        data: String,
    },
    /// An invisible placeholder used to position dynamic content.
    Anchor,
}

/// Whether the guard modifiers (`.self` / key / mouse-button) pass for a
/// listener on `node` given the simulated event `ev`.
fn guards_pass(opts: &EventOptions, node: usize, ev: &MockEvent) -> bool {
    if opts.self_only && ev.target.unwrap_or(node) != node {
        return false;
    }
    if !opts.keys.is_empty() && !ev.key.as_deref().is_some_and(|k| opts.keys.contains(&k)) {
        return false;
    }
    if !opts.buttons.is_empty() && !ev.button.is_some_and(|b| opts.buttons.contains(&b)) {
        return false;
    }
    true
}

/// Escape a text node's content: `&`, `<`, `>`.
fn escape_text(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// Escape a double-quoted attribute value: `&`, `"`.
fn escape_attribute(s: &str) -> String {
    s.replace('&', "&amp;").replace('"', "&quot;")
}

/// An in-memory DOM tree for testing. Nodes are addressed by `usize` handles.
#[derive(Clone, Default)]
pub struct MockDom {
    nodes: Rc<RefCell<Vec<NodeData>>>,
    /// Source of unique ids handed to each registered listener.
    next_listener_id: Rc<Cell<usize>>,
}

impl MockDom {
    pub fn new() -> Self {
        Self::default()
    }

    fn push(&self, data: NodeData) -> usize {
        let mut nodes = self.nodes.borrow_mut();
        nodes.push(data);
        nodes.len() - 1
    }

    /// Serialize the subtree rooted at `node` to an HTML-like string.
    /// Anchors render as nothing, so dynamic positioning stays invisible.
    /// Text content and attribute values are HTML-escaped, matching how the
    /// browser serializes nodes created via `create_text_node`/`set_attribute`.
    pub fn to_html(&self, node: usize) -> String {
        let nodes = self.nodes.borrow();
        match &nodes[node] {
            NodeData::Anchor => String::new(),
            NodeData::Text { data } => escape_text(data),
            NodeData::Element {
                tag,
                attrs,
                children,
                inner_html,
                ..
            } => {
                let mut out = format!("<{tag}");
                for (name, value) in attrs {
                    let _ = write!(out, r#" {name}="{}""#, escape_attribute(value));
                }
                out.push('>');
                if let Some(html) = inner_html {
                    // `v-html` content is inserted raw and replaces any children.
                    out.push_str(html);
                } else {
                    // Shared borrow is re-entrant across the recursion (no mut borrow).
                    for child in children {
                        out.push_str(&self.to_html(*child));
                    }
                }
                let _ = write!(out, "</{tag}>");
                out
            }
        }
    }

    /// Find the first element node (in creation order) with the given tag.
    pub fn find(&self, tag: &str) -> Option<usize> {
        let nodes = self.nodes.borrow();
        nodes.iter().position(|n| matches!(n, NodeData::Element { tag: t, .. } if t == tag))
    }

    /// Invoke listeners registered for `event` on `node` with no value and a
    /// self-targeted event (so `.self` passes, no key/button).
    pub fn dispatch(&self, node: usize, event: &str) -> DispatchOutcome {
        self.dispatch_value(node, event, "")
    }

    /// Invoke listeners registered for `event` on `node`, passing `value`
    /// (e.g. simulating typing into an input), with a self-targeted event.
    pub fn dispatch_value(&self, node: usize, event: &str, value: &str) -> DispatchOutcome {
        self.dispatch_full(node, event, value, &MockEvent::default())
    }

    /// Invoke listeners for `event` on `node` with simulated event data, so the
    /// guard modifiers (`.self` / key / mouse-button) can be exercised.
    pub fn dispatch_event(&self, node: usize, event: &str, ev: MockEvent) -> DispatchOutcome {
        self.dispatch_full(node, event, "", &ev)
    }

    /// Shared dispatch: applies `once` (removing the listener once it fires),
    /// evaluates the guard modifiers, and reports the prevent/stop modifiers of
    /// the listeners whose guards passed.
    fn dispatch_full(
        &self,
        node: usize,
        event: &str,
        value: &str,
        ev: &MockEvent,
    ) -> DispatchOutcome {
        let matched: Vec<(usize, EventOptions, Handler)> = {
            let nodes = self.nodes.borrow();
            match &nodes[node] {
                NodeData::Element { listeners, .. } => listeners
                    .iter()
                    .filter(|(_, name, _, _)| name == event)
                    .map(|(id, _, opts, handler)| (*id, *opts, Rc::clone(handler)))
                    .collect(),
                _ => Vec::new(),
            }
        };
        let mut outcome = DispatchOutcome::default();
        let mut spent: Vec<usize> = Vec::new();
        for (id, opts, handler) in matched {
            // The listener's closure "fires" (matched the event name), so a
            // `once` listener is removed even if a guard skips the handler,
            // mirroring the browser's `{ once: true }`.
            if opts.once {
                spent.push(id);
            }
            if !guards_pass(&opts, node, ev) {
                continue;
            }
            outcome.default_prevented |= opts.prevent_default;
            outcome.propagation_stopped |= opts.stop_propagation;
            handler(value);
        }
        if !spent.is_empty()
            && let NodeData::Element { listeners, .. } = &mut self.nodes.borrow_mut()[node]
        {
            listeners.retain(|(id, _, _, _)| !spent.contains(id));
        }
        outcome
    }
}

impl Backend for MockDom {
    type Node = usize;
    type Listener = usize;

    fn create_element(&self, tag: &str) -> usize {
        self.push(NodeData::Element {
            tag: tag.to_string(),
            attrs: Vec::new(),
            children: Vec::new(),
            listeners: Vec::new(),
            inner_html: None,
        })
    }

    fn create_text(&self, data: &str) -> usize {
        self.push(NodeData::Text {
            data: data.to_string(),
        })
    }

    fn create_anchor(&self) -> usize {
        self.push(NodeData::Anchor)
    }

    fn set_text(&self, node: &usize, data: &str) {
        if let NodeData::Text { data: slot } = &mut self.nodes.borrow_mut()[*node] {
            *slot = data.to_string();
        }
    }

    fn set_attribute(&self, node: &usize, name: &str, value: &str) {
        if let NodeData::Element { attrs, .. } = &mut self.nodes.borrow_mut()[*node] {
            if let Some(existing) = attrs.iter_mut().find(|(n, _)| n == name) {
                existing.1 = value.to_string();
            } else {
                attrs.push((name.to_string(), value.to_string()));
            }
        }
    }

    fn set_inner_html(&self, node: &usize, html: &str) {
        if let NodeData::Element { inner_html, .. } = &mut self.nodes.borrow_mut()[*node] {
            *inner_html = Some(html.to_string());
        }
    }

    fn append_child(&self, parent: &usize, child: &usize) {
        if let NodeData::Element { children, .. } = &mut self.nodes.borrow_mut()[*parent] {
            children.push(*child);
        }
    }

    fn insert_before(&self, parent: &usize, child: &usize, anchor: &usize) {
        if let NodeData::Element { children, .. } = &mut self.nodes.borrow_mut()[*parent] {
            // Mirror the DOM: inserting a node already in the tree moves it.
            children.retain(|c| c != child);
            let at = children.iter().position(|c| c == anchor).unwrap_or(children.len());
            children.insert(at, *child);
        }
    }

    fn remove_child(&self, parent: &usize, child: &usize) {
        if let NodeData::Element { children, .. } = &mut self.nodes.borrow_mut()[*parent] {
            children.retain(|c| c != child);
        }
    }

    fn add_event_listener(
        &self,
        node: &usize,
        event: &str,
        options: EventOptions,
        handler: Rc<dyn Fn(&str)>,
    ) -> usize {
        let id = self.next_listener_id.get();
        self.next_listener_id.set(id + 1);
        if let NodeData::Element { listeners, .. } = &mut self.nodes.borrow_mut()[*node] {
            listeners.push((id, event.to_string(), options, handler));
        }
        id
    }

    fn remove_event_listener(&self, node: &usize, listener: usize) {
        if let NodeData::Element { listeners, .. } = &mut self.nodes.borrow_mut()[*node] {
            listeners.retain(|(id, _, _, _)| *id != listener);
        }
    }
}
