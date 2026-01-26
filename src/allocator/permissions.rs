use crate::token::hierarchy::Permission;

/// A permission marker indicating the bootstrap phase.
/// This permission allows carving initial memory but restricts
/// other runtime operations that might depend on fully initialized state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BootstrapPermission;

impl Permission for BootstrapPermission {}
