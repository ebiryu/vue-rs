use std::rc::Rc;

use vue_rs_reactive::effect;

use crate::backend::Backend;

/// Builder for an element node. Reactive bindings (`dyn_text` / `dyn_attr`)
/// install an effect so the node updates whenever a read reactive value changes.
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

    /// Attach an event listener.
    pub fn on(self, event: &str, handler: impl Fn() + 'static) -> Self {
        self.backend
            .add_event_listener(&self.node, event, Rc::new(handler));
        self
    }

    /// Finish building and return the node.
    pub fn finish(self) -> B::Node {
        self.node
    }
}
