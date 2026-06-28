use std::rc::Rc;

use crate::backend::Backend;

/// A typed, cloneable slot builder. Each component gets a generated
/// `NameSlots<B>` struct with one `Option<SlotFn<B, T>>` field per slot the
/// parent may fill; scoped slots use their declared payload type `T`, plain
/// slots use `T = ()`. The parent supplies a plain `impl Fn(B, T) -> B::Node`
/// closure to the struct's `with_<name>` builder (whose signature names `T`, so
/// the closure needs no annotation); that builder wraps it in a `SlotFn`, so the
/// slot can be rendered — and reused across dynamic regions — by cloning. The
/// payload type is checked at compile time, and any subset of slots may be
/// provided (the rest stay `None`, rendering the slot's fallback).
pub struct SlotFn<B: Backend, T> {
    builder: Rc<dyn Fn(B, T) -> B::Node>,
}

impl<B: Backend, T> Clone for SlotFn<B, T> {
    fn clone(&self) -> Self {
        Self {
            builder: Rc::clone(&self.builder),
        }
    }
}

impl<B: Backend, T> SlotFn<B, T> {
    pub fn new(builder: impl Fn(B, T) -> B::Node + 'static) -> Self {
        Self {
            builder: Rc::new(builder),
        }
    }

    /// Build the slot's content with `data`.
    pub fn render(&self, backend: B, data: T) -> B::Node {
        (self.builder)(backend, data)
    }
}
