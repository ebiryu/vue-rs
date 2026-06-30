use std::cell::RefCell;
use std::rc::Rc;

use crate::backend::Backend;

/// A handle to a template ref (the `ref="name"` directive): a shared, nullable
/// slot that holds the DOM node of the element it is bound to.
///
/// Construct one with [`template_ref`] in a component's `<script>`, bind it in
/// the template with `ref="name"`, then read the node with [`get`](Self::get)
/// once the element is mounted (e.g. in `on_mounted` or an event handler). The
/// slot is populated while the bound element is mounted and cleared to `None`
/// when the element's owning reactive scope is disposed.
pub struct TemplateRef<B: Backend> {
    node: Rc<RefCell<Option<B::Node>>>,
}

impl<B: Backend> TemplateRef<B> {
    /// The bound element's node, or `None` before it is mounted (or after its
    /// owning scope is disposed).
    pub fn get(&self) -> Option<B::Node> {
        self.node.borrow().clone()
    }

    /// Store the bound element's node. Called by [`crate::El::node_ref`].
    pub(crate) fn set(&self, node: Option<B::Node>) {
        *self.node.borrow_mut() = node;
    }

    /// The shared slot, so a cleanup can clear it on disposal.
    pub(crate) fn slot(&self) -> Rc<RefCell<Option<B::Node>>> {
        self.node.clone()
    }
}

// Derived `Clone` would demand `B: Clone`; the `Rc` makes the handle cheap to
// clone regardless, so implement it by hand to clone only the shared slot.
impl<B: Backend> Clone for TemplateRef<B> {
    fn clone(&self) -> Self {
        Self {
            node: self.node.clone(),
        }
    }
}

/// Create an empty [`TemplateRef`] for binding with `ref="name"`. The backend
/// type is inferred from the `ref=` binding site, so a component's `<script>`
/// can write `let name = template_ref();` with no annotation.
pub fn template_ref<B: Backend>() -> TemplateRef<B> {
    TemplateRef {
        node: Rc::new(RefCell::new(None)),
    }
}
