use std::alloc;
use std::cell::UnsafeCell;
use std::mem::MaybeUninit;
use std::ptr;
use std::slice;
use std::sync::atomic::AtomicPtr;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering::{AcqRel, Acquire, Release};

use crate::node::Node;

use super::raw::SLOTS;

pub struct Chunk<T> {
    /// should be the size of 2 * `SLOTS`
    active: AtomicUsize,
    /// should be the size of [`usize::BITS`] / `2`
    slots: [Slot<T>; SLOTS],
}

impl<T> Drop for Chunk<T> {
    fn drop(&mut self) {
        for slot in 0..SLOTS {
            // NOTE: middle shouldn't really be encountered here. maybe should still check
            let state = SlotState::read(*self.active.get_mut(), slot);
            if matches!(state, SlotState::Init) {
                // SAFETY: slot is within 0..SLOTS
                unsafe {
                    ptr::drop_in_place((*self.slots.get_unchecked(slot).slot.get()).as_mut_ptr());
                }
            }
        }
    }
}

// TODO: test the Middle state and how its handled

/// Describes the slot's state
#[derive(Debug, PartialEq, Eq)]
enum SlotState {
    /// slot has not been set
    Uninit = 0b00,
    /// slot is in the middle of being set
    Middle = 0b01,
    /// slot has been set
    Init = 0b11,
}

impl SlotState {
    const UNINIT: usize = Self::Uninit as usize;
    const MIDDLE: usize = Self::Middle as usize;
    const INIT: usize = Self::Init as usize;

    fn read(data: usize, slot: usize) -> Self {
        match (data >> (slot * 2)) & 0b11 {
            Self::INIT => Self::Init,
            Self::MIDDLE => Self::Middle,
            Self::UNINIT => Self::Uninit,
            n => unreachable!("should not encounter this: {n}"),
        }
    }

    const fn set_slot(self, slot: usize) -> usize {
        (self as usize) << (slot * 2)
    }
}

impl<T> Chunk<T> {
    // get node at slot
    //
    // # Safety
    //
    // `slot` must be less than `64`
    pub unsafe fn get(&self, slot: usize) -> Option<&Node<T>> {
        self.acquire(slot).then(|| unsafe { self.read(slot) })
    }

    /// gets node
    ///
    /// # Safety
    ///
    /// `slot` must be initialized
    unsafe fn read(&self, slot: usize) -> &Node<T> {
        unsafe { self.slots.get_unchecked(slot).read() }
    }

    /// write the given node to the slot
    ///
    /// # Safety
    ///
    /// The slot must be uninitialized
    pub unsafe fn write_root(&self, slot: usize, node: Node<T>) -> &Node<T> {
        let node = unsafe { self.slots.get_unchecked(slot).root(node) };
        self.active
            .fetch_or(SlotState::Init.set_slot(slot), Release);
        node
    }

    /// write the given node to the slot
    ///
    /// # Safety
    ///
    /// The slot must be uninitialized
    pub unsafe fn write(
        &self,
        slot: usize,
        node: Node<T>,
        index: super::Index,
        parent: &Node<T>,
    ) -> &Node<T> {
        // NOTE: should reconsider which ordering to use
        self.active
            .fetch_or(SlotState::Middle.set_slot(slot), Release);
        let node = unsafe { self.slots.get_unchecked(slot).write(node, index, parent) };
        self.active
            .fetch_or(SlotState::Init.set_slot(slot), Release);
        node
    }

    fn acquire(&self, slot: usize) -> bool {
        match SlotState::read(self.active.load(Acquire), slot) {
            SlotState::Middle => {
                self.spin(slot);
                true
            }
            SlotState::Uninit => false,
            SlotState::Init => true,
        }
    }

    #[cold]
    #[inline(never)]
    fn spin(&self, slot: usize) {
        // NOTE: should look into creating a limit
        loop {
            match SlotState::read(self.active.load(Acquire), slot) {
                SlotState::Middle => std::hint::spin_loop(),
                SlotState::Init => break,
                SlotState::Uninit => {
                    unreachable!("state should never go from Middle to Uninit")
                }
            }
        }
    }

    /// Race to initialize a bucket.
    ///
    /// # Safety
    ///
    /// The provided length must be non-zero & the correct amount for the given bucket
    pub unsafe fn alloc_bucket(bucket: &AtomicPtr<Self>, len: usize) -> *mut Self {
        let chunks = unsafe { Self::alloc(len) };

        match bucket.compare_exchange(ptr::null_mut(), chunks, Release, Acquire) {
            Ok(_) => chunks,
            Err(found) => {
                unsafe { Self::dealloc(chunks, len) };
                found
            }
        }
    }

    /// Allocate an array of chunks of the specified length.
    ///
    /// # Safety
    ///
    /// The provided length must be non-zero.
    pub unsafe fn alloc(len: usize) -> *mut Self {
        let layout = alloc::Layout::array::<Self>(len).unwrap();
        let ptr = unsafe { alloc::alloc_zeroed(layout) };
        if ptr.is_null() {
            alloc::handle_alloc_error(layout);
        }
        ptr.cast::<Self>()
    }

    /// Deallocate a bucket of the specified capacity.
    ///
    /// # Safety
    ///
    /// The safety requirements of `slice::from_raw_parts_mut` and
    /// `Box::from_raw`. The pointer must be a valid, owned pointer
    /// to an array of entries of the provided length.
    pub unsafe fn dealloc(chunks: *mut Self, len: usize) {
        drop(unsafe { Box::from_raw(slice::from_raw_parts_mut(chunks, len)) });
    }
}

pub struct Slot<T> {
    slot: UnsafeCell<MaybeUninit<Node<T>>>,
}

impl<T> Slot<T> {
    /// gets slot
    ///
    /// # Safety
    ///
    /// `slot` must be initialized
    unsafe fn read(&self) -> &Node<T> {
        unsafe { (*self.slot.get()).assume_init_ref() }
    }

    /// write node to slot
    ///
    /// # Safety
    ///
    /// The slot must be uninitialized
    unsafe fn root(&self, node: Node<T>) -> &Node<T> {
        println!("{:p}", self.slot.get());
        unsafe { (*self.slot.get()).write(node) }
    }

    /// write node to slot
    ///
    /// # Safety
    ///
    /// The slot must be uninitialized
    unsafe fn write(&self, node: Node<T>, index: super::Index, parent: &Node<T>) -> &Node<T> {
        let node = unsafe { (*self.slot.get()).write(node) };
        *node.next.0.get_mut() = parent.child.0.swap(index.0.get(), AcqRel);
        node
    }
}
