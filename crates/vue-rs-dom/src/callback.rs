use std::rc::Rc;

/// A cloneable event callback used for component emits (`@event` on a component).
/// Wraps an `Fn(T)`; the child calls it to emit a value to the parent.
pub struct Callback<T>(Rc<dyn Fn(T)>);

impl<T> Clone for Callback<T> {
    fn clone(&self) -> Self {
        Self(Rc::clone(&self.0))
    }
}

impl<T> Callback<T> {
    /// Wrap a handler.
    pub fn new(handler: impl Fn(T) + 'static) -> Self {
        Self(Rc::new(handler))
    }

    /// Invoke the callback with `value`.
    pub fn call(&self, value: T) {
        (self.0)(value)
    }
}

impl<T, F: Fn(T) + 'static> From<F> for Callback<T> {
    fn from(handler: F) -> Self {
        Self::new(handler)
    }
}

impl<T> Default for Callback<T> {
    /// A no-op callback, so an optional emit prop (`#[prop(default)]`) the parent
    /// leaves unlistened simply drops emitted values.
    fn default() -> Self {
        Self::new(|_| {})
    }
}
