//! Lazy/once initialization primitives (token-gated).

pub mod ghost_lazy_cell;
pub mod ghost_lazy_lock;
pub mod ghost_once_cell;

pub use ghost_lazy_cell::GhostLazyCell;
pub use ghost_lazy_lock::GhostLazyLock;
pub use ghost_once_cell::GhostOnceCell;









