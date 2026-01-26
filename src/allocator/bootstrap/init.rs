use std::sync::OnceLock;
use super::arena::BootstrapArena;
use crate::allocator::constants::BOOTSTRAP_RESERVE_SIZE;

static BOOTSTRAP_ARENA: OnceLock<BootstrapArena> = OnceLock::new();

/// Initializes the bootstrap subsystem.
///
/// Acquires the initial virtual memory region.
/// Returns a reference to the global bootstrap arena.
pub fn bootstrap() -> Result<&'static BootstrapArena, &'static str> {
    if let Some(arena) = BOOTSTRAP_ARENA.get() {
        return Ok(arena);
    }

    let arena = BootstrapArena::new(BOOTSTRAP_RESERVE_SIZE)
        .ok_or("Failed to allocate bootstrap memory region")?;

    match BOOTSTRAP_ARENA.set(arena) {
        Ok(_) => Ok(BOOTSTRAP_ARENA.get().unwrap()),
        Err(_) => {
            // Race condition: another thread initialized it.
            // The local 'arena' will be dropped here, releasing the mmap
            // (since BootstrapArena implements Drop).
            Ok(BOOTSTRAP_ARENA.get().unwrap())
        }
    }
}
