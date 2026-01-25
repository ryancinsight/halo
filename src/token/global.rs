use crate::token::{GhostToken, InvariantLifetime};
use std::sync::{Mutex, OnceLock};

/// A zero-sized marker type representing the global brand.
///
/// This type can be used to explicitly name the global brand in type signatures,
/// although the global token uses the `'static` lifetime directly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StaticBrand;

/// Returns a reference to the global static ghost token.
///
/// This token is created once and leaked, ensuring it lives for the entire
/// duration of the program. It allows concurrent immutable access across threads.
///
/// # Performance
///
/// Accessing the static token is extremely cheap (checking a `OnceLock`),
/// and subsequent accesses are essentially free references.
pub fn static_token() -> &'static GhostToken<'static> {
    static TOKEN: OnceLock<GhostToken<'static>> = OnceLock::new();
    // Note: This relies on `GhostToken` having a constructor accessible here.
    // We will ensure `GhostToken` exposes a `pub(crate)` way to construct it
    // or we update visibility in `mod.rs`.
    // SAFETY: We are creating the singleton static token. This is safe because
    // it is only done once (via OnceLock) and the brand is 'static.
    TOKEN.get_or_init(|| unsafe { GhostToken::from_invariant(InvariantLifetime::default()) })
}

/// A global mutex to enforce exclusive access for the mutable variant.
static MUTABLE_GUARD: Mutex<()> = Mutex::new(());

/// Executes a closure with access to the global static token.
///
/// This allows immutable access to the token, which is concurrent-safe.
///
/// # Example
///
/// ```rust
/// use halo::token::{static_token, with_static_token};
///
/// with_static_token(|token| {
///     // use token for read-only access to global branded cells
/// });
/// ```
pub fn with_static_token<F, R>(f: F) -> R
where
    F: FnOnce(&'static GhostToken<'static>) -> R,
{
    f(static_token())
}

/// Executes a closure with exclusive mutable access to the global static token.
///
/// # Safety
///
/// This function is unsafe because it creates a `&mut GhostToken<'static>` which
/// can alias with the `&'static GhostToken<'static>` returned by `static_token()`.
///
/// The caller must ensure that this function is **only** called during
/// initialization or bootstrapping phases where no other threads are accessing
/// the static token (either mutably or immutably).
///
/// If any other reference to the static token exists and is being used
/// concurrently, calling this function causes undefined behavior (data races).
///
/// # Synchronization
///
/// This function uses a global `Mutex` to serialize calls to `with_static_token_mut`
/// relative to each other. It does *not* synchronize against `static_token()` or
/// `with_static_token`.
pub unsafe fn with_static_token_mut<F, R>(f: F) -> R
where
    F: FnOnce(&mut GhostToken<'static>) -> R,
{
    // Serialize mutable access requests.
    let _guard = MUTABLE_GUARD.lock().unwrap();

    // Create a temporary mutable token.
    // SAFETY: The caller guarantees no other references are in use.
    // We are the only thread in this function due to the mutex.
    // We treat `from_invariant` as unsafe to acknowledge we are forging a token.
    let mut token = unsafe { GhostToken::from_invariant(InvariantLifetime::default()) };
    f(&mut token)
}
