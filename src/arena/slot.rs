use std::cell::UnsafeCell;
use std::mem::MaybeUninit;
use std::sync::atomic::AtomicU8;
use std::sync::atomic::Ordering::{Acquire, Release};

use crate::Node;

// NOTE: could move uninit to node.value

pub struct Slot<T> {
    state: AtomicU8,
    slot: UnsafeCell<MaybeUninit<Node<T>>>,
}

impl<T> Drop for Slot<T> {
    fn drop(&mut self) {
        if matches!(self.state_mut(), State::Active) {
            // SAFETY: slot is confirmed to be init
            unsafe { self.slot.get_mut().assume_init_drop() };
        }
    }
}

impl<T> Slot<T> {
    /// get the node if it is init
    pub fn get(&self) -> Option<&Node<T>> {
        // SAFETY: state is checked
        self.acquire().then(|| unsafe { self.get_unchecked() })
    }

    /// gets slot
    ///
    /// # Safety
    ///
    /// `slot` must be initialized
    pub unsafe fn get_unchecked(&self) -> &Node<T> {
        // SAFETY: upheld by caller
        unsafe { (*self.slot.get()).assume_init_ref() }
    }

    /// write the given node to the slot
    ///
    /// # Safety
    ///
    /// The slot must be uninitialized, `parent` should be from the arena
    /// this slot belongs to
    pub unsafe fn write(&self, node: Node<T>, parent: Option<&crate::Node<T>>) -> &Node<T> {
        self.state.store(State::Middle as u8, Release); //could be relaxed
        // SAFETY: upheld by caller
        unsafe { (*self.slot.get()).write(node) };
        if let Some(parent) = parent {
            unsafe { parent.add_child(UnsafeCell::raw_get(&raw const self.slot).cast()) };
        }
        self.state.store(State::Active as u8, Release);

        // SAFETY: has been init above
        unsafe { self.get_unchecked() }
    }

    fn acquire(&self) -> bool {
        match self.state() {
            State::Uninit => false,
            State::Middle => self.spin(),
            State::Active => true,
        }
    }

    #[cold]
    fn spin(&self) -> bool {
        // maybe should use exponential backoff
        loop {
            // could use a relaxed ordering here, confirming with Acquire
            match self.state() {
                State::Uninit => break false,
                State::Middle => std::hint::spin_loop(),
                State::Active => break true,
            }
        }
    }

    fn state(&self) -> State {
        self.state.load(Acquire).into()
    }

    fn state_mut(&mut self) -> State {
        (*self.state.get_mut()).into()
    }
}

enum State {
    /// The slot is uninit, it must not be read.
    Uninit = 0b0000,
    /// The slot is being init, it may be read under sound conditions.
    Middle = 0b0001,
    /// the slot is init, it can be read.
    Active = 0b0011,
}

impl From<u8> for State {
    fn from(value: u8) -> Self {
        #[expect(clippy::match_same_arms)]
        match value {
            0b0000 => Self::Uninit,
            0b0001 => Self::Middle,
            0b0011 => Self::Active,
            _ => Self::Uninit,
        }
    }
}
