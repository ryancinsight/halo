use core::mem::MaybeUninit;

/// Layout note: store `value` first; keep `is_init` in tail padding.
pub(super) struct Inner<T> {
    pub(super) value: MaybeUninit<T>,
    pub(super) is_init: bool,
}






