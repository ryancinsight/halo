//! Debug-only mathematical assertion helpers.
//!
//! The DAG module uses these helpers to keep invariants explicit while ensuring
//! release builds remain unaffected.

/// Debug-asserts a mathematical invariant with a message.
#[inline(always)]
pub(crate) fn math_assert_msg(condition: bool, message: &str) {
    debug_assert!(condition, "Mathematical invariant violated: {}", message);
}



