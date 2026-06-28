use std::cell::{Cell, RefCell};
use std::fmt::Write as _;
use std::rc::Rc;

use crate::backend::Backend;

/// An event handler receiving the event's value.
type Handler = Rc<dyn Fn(&str)>;
/// A registered listener: a unique id (for removal), its event name, and handler.
type Listener = (usize, String, Handler);

enum NodeData {
    Element {
        tag: String,
        attrs: Vec<(String, String)>,
        children: Vec<usize>,
        listeners: Vec<Listener>,
    },
    Text {
        data: String,
    },
    /// An invisible placeholder used to position dynamic content.
    Anchor,
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
                ..
            } => {
                let mut out = format!("<{tag}");
                for (name, value) in attrs {
                    let _ = write!(out, r#" {name}="{}""#, escape_attribute(value));
                }
                out.push('>');
                // Shared borrow is re-entrant across the recursion (no mut borrow).
                for child in children {
                    out.push_str(&self.to_html(*child));
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

    /// Invoke listeners registered for `event` on `node` with no value.
    pub fn dispatch(&self, node: usize, event: &str) {
        self.dispatch_value(node, event, "");
    }

    /// Invoke listeners registered for `event` on `node`, passing `value`
    /// (e.g. simulating typing into an input).
    pub fn dispatch_value(&self, node: usize, event: &str, value: &str) {
        let handlers: Vec<Handler> = {
            let nodes = self.nodes.borrow();
            match &nodes[node] {
                NodeData::Element { listeners, .. } => listeners
                    .iter()
                    .filter(|(_, name, _)| name == event)
                    .map(|(_, _, handler)| Rc::clone(handler))
                    .collect(),
                _ => Vec::new(),
            }
        };
        for handler in handlers {
            handler(value);
        }
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
        handler: Rc<dyn Fn(&str)>,
    ) -> usize {
        let id = self.next_listener_id.get();
        self.next_listener_id.set(id + 1);
        if let NodeData::Element { listeners, .. } = &mut self.nodes.borrow_mut()[*node] {
            listeners.push((id, event.to_string(), handler));
        }
        id
    }

    fn remove_event_listener(&self, node: &usize, listener: usize) {
        if let NodeData::Element { listeners, .. } = &mut self.nodes.borrow_mut()[*node] {
            listeners.retain(|(id, _, _)| *id != listener);
        }
    }
}
