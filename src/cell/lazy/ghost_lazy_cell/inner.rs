use core::mem::{ManuallyDrop, MaybeUninit};

/// Layout note: `value` is placed before `is_init` to keep the flag in tail padding.
pub(super) struct Inner<T, F> {
    pub(super) init: ManuallyDrop<F>,
    pub(super) value: MaybeUninit<T>,
    pub(super) is_init: bool,
}






