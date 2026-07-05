//! Fine-grained reactivity core for vue-rs.
//!
//! Public API: [`signal`], [`computed`], [`effect`], the scope helpers
//! [`create_root`] / [`on_cleanup`], and the handle types [`Signal`] / [`Memo`]
//! / [`ReadSignal`] (a read-only view, used for props).

mod runtime;

pub use runtime::{
    batch, clear_scheduler, computed, computed_raw, create_root, create_root_detached, effect,
    flush_jobs, flush_mounted, next_tick, on_cleanup, on_mounted, on_unmounted, provide_context,
    run_in_child_scope, set_scheduler, signal, signal_raw, use_context, watch, watch_immediate,
    watch_with, writable_computed, Flush, Memo, ReadSignal, RootDisposer, Signal, WatchOptions,
    WritableMemo,
};
