use std::rc::Rc;

/// Modifiers that change how an event listener behaves (the `@event.*` directive
/// modifiers). Applied by the backend when the event fires; the handler itself is
/// unaware of them.
///
/// Fields fall in two groups. The first runs the handler then acts (or changes
/// registration): `prevent_default`, `stop_propagation`, `once`, `capture`,
/// `passive`. The rest are *guards* — when set, the handler runs only if the
/// event matches: `self_only` (target is this element), `keys` (`event.key` is
/// one of these), `buttons` (`event.button` is one of these). An empty `keys` /
/// `buttons` means no filtering on that axis.
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub struct EventOptions {
    /// Call the event's `preventDefault()` before invoking the handler.
    pub prevent_default: bool,
    /// Call the event's `stopPropagation()` before invoking the handler.
    pub stop_propagation: bool,
    /// Detach the listener after it fires once.
    pub once: bool,
    /// Register the listener for the capture phase.
    pub capture: bool,
    /// Register the listener as passive (it will not call `preventDefault`).
    pub passive: bool,
    /// Run only when the event's target is the element itself (the `.self`
    /// modifier), not a descendant.
    pub self_only: bool,
    /// Run only when `event.key` matches one of these (the key modifiers, e.g.
    /// `@keyup.enter`). Empty means no key filtering.
    pub keys: &'static [&'static str],
    /// Run only when `event.button` matches one of these (the mouse-button
    /// modifiers, e.g. `@click.right`). Empty means no button filtering.
    pub buttons: &'static [u16],
}

/// Abstraction over a tree of DOM-like nodes. Implemented by [`crate::MockDom`]
/// for tests and (with the `web` feature) by `WebDom` over `web-sys`.
///
/// Handles must be `Clone + 'static` so reactive effects can capture them.
pub trait Backend: Clone + 'static {
    type Node: Clone + 'static;
    /// Handle to an attached event listener, used to detach it later. Owns
    /// whatever must be kept alive while the listener is registered (e.g. the
    /// JS closure for the web backend).
    type Listener: 'static;

    fn create_element(&self, tag: &str) -> Self::Node;
    fn create_text(&self, data: &str) -> Self::Node;
    /// Create an empty placeholder used to anchor dynamic content in the tree.
    fn create_anchor(&self) -> Self::Node;
    fn set_text(&self, node: &Self::Node, data: &str);
    fn set_attribute(&self, node: &Self::Node, name: &str, value: &str);
    /// Replace the element's children with raw, unparsed-by-us markup (the
    /// `v-html` directive). The backend inserts `html` without escaping.
    fn set_inner_html(&self, node: &Self::Node, html: &str);
    fn append_child(&self, parent: &Self::Node, child: &Self::Node);
    /// Insert `child` immediately before `anchor` within `parent`.
    fn insert_before(&self, parent: &Self::Node, child: &Self::Node, anchor: &Self::Node);
    fn remove_child(&self, parent: &Self::Node, child: &Self::Node);
    /// Attach an event listener. The handler receives the event's value (e.g. an
    /// input's text), or an empty string for events that carry no value. Returns
    /// a handle that must be passed to [`Backend::remove_event_listener`] to
    /// detach the listener and release its resources. `options` carries any
    /// event modifiers (prevent/stop/once) the backend should apply.
    fn add_event_listener(
        &self,
        node: &Self::Node,
        event: &str,
        options: EventOptions,
        handler: Rc<dyn Fn(&str)>,
    ) -> Self::Listener;
    /// Detach a listener previously attached with [`Backend::add_event_listener`].
    fn remove_event_listener(&self, node: &Self::Node, listener: Self::Listener);
}
