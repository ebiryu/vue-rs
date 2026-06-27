use std::collections::HashMap;
use std::rc::Rc;

use crate::backend::Backend;

type SlotFn<B> = Rc<dyn Fn(B) -> <B as Backend>::Node>;

/// The slot content a parent passes to a component, keyed by slot name. The
/// unnamed (default) slot uses the name `"default"`.
pub struct Slots<B: Backend> {
    slots: HashMap<&'static str, SlotFn<B>>,
}

impl<B: Backend> Default for Slots<B> {
    fn default() -> Self {
        Self {
            slots: HashMap::new(),
        }
    }
}

impl<B: Backend> Slots<B> {
    pub fn new() -> Self {
        Self::default()
    }

    /// Like [`new`](Self::new), but pins the backend type from a value so slot
    /// builder closures can infer their parameter type.
    pub fn for_backend(_backend: &B) -> Self {
        Self::new()
    }

    /// Register the builder for a named slot.
    pub fn with(mut self, name: &'static str, builder: impl Fn(B) -> B::Node + 'static) -> Self {
        self.slots.insert(name, Rc::new(builder));
        self
    }

    /// Build the content for `name`, or `None` if the parent did not provide it.
    pub fn render(&self, name: &str, backend: B) -> Option<B::Node> {
        self.slots.get(name).map(|builder| builder(backend))
    }
}
