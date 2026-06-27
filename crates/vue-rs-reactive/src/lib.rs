//! Fine-grained reactivity core for vue-rs.
//!
//! Public API: [`signal`], [`computed`], [`effect`], the scope helpers
//! [`create_root`] / [`on_cleanup`], and the handle types [`Signal`] / [`Memo`].

mod runtime;

pub use runtime::{
    batch, computed, create_root, create_root_detached, effect, flush_mounted, on_cleanup,
    on_mounted, on_unmounted, provide_context, run_in_child_scope, signal, use_context, Memo,
    RootDisposer, Signal,
};
