use std::cell::RefCell;
use std::fmt::Write as _;
use std::rc::Rc;

use crate::backend::Backend;

/// An event handler receiving the event's value.
type Handler = Rc<dyn Fn(&str)>;
/// An event handler keyed by event name.
type Listener = (String, Handler);

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

/// An in-memory DOM tree for testing. Nodes are addressed by `usize` handles.
#[derive(Clone, Default)]
pub struct MockDom {
    nodes: Rc<RefCell<Vec<NodeData>>>,
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
    pub fn to_html(&self, node: usize) -> String {
        let nodes = self.nodes.borrow();
        match &nodes[node] {
            NodeData::Anchor => String::new(),
            NodeData::Text { data } => data.clone(),
            NodeData::Element {
                tag,
                attrs,
                children,
                ..
            } => {
                let mut out = format!("<{tag}");
                for (name, value) in attrs {
                    let _ = write!(out, r#" {name}="{value}""#);
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
                    .filter(|(name, _)| name == event)
                    .map(|(_, handler)| Rc::clone(handler))
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

    fn add_event_listener(&self, node: &usize, event: &str, handler: Rc<dyn Fn(&str)>) {
        if let NodeData::Element { listeners, .. } = &mut self.nodes.borrow_mut()[*node] {
            listeners.push((event.to_string(), handler));
        }
    }
}
