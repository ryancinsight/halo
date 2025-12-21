use core::mem::ManuallyDrop;

#[repr(u8)]
#[derive(Copy, Clone, Eq, PartialEq)]
pub(super) enum State {
    Uninit = 0,
    Init = 1,
}

pub(super) union Slot<T, F> {
    pub(super) init: ManuallyDrop<F>,
    pub(super) value: ManuallyDrop<T>,
}

/// Layout note: `slot` is first to avoid padding-before-slot for its alignment.
pub(super) struct Inner<T, F> {
    pub(super) slot: Slot<T, F>,
    pub(super) state: State,
}






