use std::rc::Rc;

/// Abstraction over a tree of DOM-like nodes. Implemented by [`crate::MockDom`]
/// for tests and (with the `web` feature) by `WebDom` over `web-sys`.
///
/// Handles must be `Clone + 'static` so reactive effects can capture them.
pub trait Backend: Clone + 'static {
    type Node: Clone + 'static;

    fn create_element(&self, tag: &str) -> Self::Node;
    fn create_text(&self, data: &str) -> Self::Node;
    /// Create an empty placeholder used to anchor dynamic content in the tree.
    fn create_anchor(&self) -> Self::Node;
    fn set_text(&self, node: &Self::Node, data: &str);
    fn set_attribute(&self, node: &Self::Node, name: &str, value: &str);
    fn append_child(&self, parent: &Self::Node, child: &Self::Node);
    /// Insert `child` immediately before `anchor` within `parent`.
    fn insert_before(&self, parent: &Self::Node, child: &Self::Node, anchor: &Self::Node);
    fn remove_child(&self, parent: &Self::Node, child: &Self::Node);
    /// Attach an event listener. The handler receives the event's value (e.g. an
    /// input's text), or an empty string for events that carry no value.
    fn add_event_listener(&self, node: &Self::Node, event: &str, handler: Rc<dyn Fn(&str)>);
}
