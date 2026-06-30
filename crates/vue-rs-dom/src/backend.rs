use std::rc::Rc;

/// Modifiers that change how an event listener behaves (the `@event.*` directive
/// modifiers). Applied by the backend when the event fires; the handler itself is
/// unaware of them.
///
/// Fields fall in two groups. The first runs the handler then acts (or changes
/// registration): `prevent_default`, `stop_propagation`, `once`, `capture`,
/// `passive`. The rest are *guards* — when set, the handler runs only if the
/// event matches: `self_only` (target is this element), `keys` (`event.key` is
/// one of these), `buttons` (`event.button` is one of these), the system-modifier
/// flags `ctrl`/`alt`/`shift`/`meta` (the matching modifier key is held), and
/// `exact` (only the requested system modifiers are held, none other). An empty
/// `keys` / `buttons` means no filtering on that axis.
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
    /// Run only when the Control key is held (the `.ctrl` modifier).
    pub ctrl: bool,
    /// Run only when the Alt key is held (the `.alt` modifier).
    pub alt: bool,
    /// Run only when the Shift key is held (the `.shift` modifier).
    pub shift: bool,
    /// Run only when the Meta key is held (the `.meta` modifier).
    pub meta: bool,
    /// Require *exactly* the requested system modifiers (the `.exact` modifier):
    /// every system modifier not set among `ctrl`/`alt`/`shift`/`meta` must be
    /// absent for the handler to run.
    pub exact: bool,
}

impl EventOptions {
    /// Whether the system-modifier guards (`.ctrl`/`.alt`/`.shift`/`.meta` and
    /// `.exact`) pass given which modifier keys are held, in
    /// `[ctrl, alt, shift, meta]` order. Each requested modifier must be held;
    /// with `exact`, no modifier outside the requested set may be held.
    pub fn system_modifiers_pass(&self, held: [bool; 4]) -> bool {
        let required = [self.ctrl, self.alt, self.shift, self.meta];
        for i in 0..4 {
            if required[i] && !held[i] {
                return false;
            }
            if self.exact && !required[i] && held[i] {
                return false;
            }
        }
        true
    }
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
    /// Set a DOM property (the `:name.prop` binding), e.g. `node.value = "x"`,
    /// rather than an attribute.
    fn set_property(&self, node: &Self::Node, name: &str, value: &str);
    /// Set a boolean DOM property, e.g. a checkbox's `node.checked = true` (the
    /// `v-model` binding on `<input type="checkbox">`). A boolean cannot go through
    /// [`Backend::set_property`] because a non-empty string is always truthy.
    fn set_bool_property(&self, node: &Self::Node, name: &str, value: bool);
    /// Remove a previously-set attribute. Used when a dynamic attribute argument
    /// (`:[key]`) changes its name, so the old attribute does not linger.
    fn remove_attribute(&self, node: &Self::Node, name: &str);
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
