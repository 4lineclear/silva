use std::cell::UnsafeCell;
use std::mem::MaybeUninit;
use std::ptr;
use std::sync::atomic::AtomicU8;
use std::sync::atomic::Ordering::{AcqRel, Acquire, Release};

use crate::Node;

pub struct Slot<T> {
    state: AtomicU8,
    slot: UnsafeCell<MaybeUninit<Node<T>>>,
}

impl<T> Drop for Slot<T> {
    fn drop(&mut self) {
        if matches!(self.state_mut(), State::Active) {
            // SAFETY: slot is confirmed to be init
            unsafe {
                ptr::drop_in_place(self.slot.get_mut().as_mut_ptr());
            }
        }
    }
}

impl<T> Slot<T> {
    /// get node at slot
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
    pub unsafe fn write(&self, node: Node<T>, parent: Option<&Node<T>>) -> &Node<T> {
        self.state.store(State::Middle as u8, Release);

        // SAFETY: upheld by caller
        let node = if let Some(parent) = parent {
            unsafe { self.write_node(node, parent) }
        } else {
            unsafe { self.write_root(node) }
        };

        self.state.store(State::Active as u8, Release);
        node
    }

    /// write node to slot
    ///
    /// # Safety
    ///
    /// The slot must be uninitialized
    unsafe fn write_root(&self, node: Node<T>) -> &Node<T> {
        // SAFETY: upheld by caller
        unsafe { (*self.slot.get()).write(node) }
    }

    /// write node to slot
    ///
    /// # Safety
    ///
    /// The slot must be uninitialized, parent & node should end up in the same arena
    #[inline(never)]
    unsafe fn write_node(&self, node: Node<T>, parent: &Node<T>) -> &Node<T> {
        // SAFETY: upheld by caller
        let node = unsafe { (*self.slot.get()).write(node) };
        let _ = parent.child.fetch_update(AcqRel, Acquire, |value| {
            node.next = ptr::NonNull::new(value);
            Some(ptr::from_mut(node))
        });

        node
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

// could use the below function for spinning

// #[inline]
// pub fn spin(spin: u32) {
//     const SPIN_LIMIT: u32 = 8;
//     const MAX_LIMIT: u32 = 1 << SPIN_LIMIT;
//
//     match spin {
//         ..=SPIN_LIMIT => {
//             for _ in 0..spin.pow(2) {
//                 std::hint::spin_loop()
//             }
//         }
//         ..=MAX_LIMIT => {
//             std::thread::yield_now();
//         }
//         _ => {
//             panic!("max spin reached");
//         }
//     }
// }
