use std::ptr;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering::{Acquire, Relaxed};

use crate::Index;
use crate::Node;

use super::bucket::Bucket;
use super::slot::Slot;

/// The base for `slot_cap`
pub const SLOTS: usize = usize::BITS as usize;
/// The number of skipped slots
pub const ZERO_SLOT: usize = SLOTS - 1;
/// The number of skipped buckets
pub const ZERO_BUCKET: usize = SLOTS - ZERO_SLOT.leading_zeros() as usize;
/// The number of buckets to be used
pub const BUCKETS: usize = SLOTS - 1 - ZERO_BUCKET;
/// The inclusive max index(slot) able to be stored
pub const MAX_INDEX: usize = isize::MAX as usize - SLOTS;

pub struct Arena<T> {
    buckets: [Bucket<Slot<T>>; BUCKETS],
    index: AtomicUsize,
    count: AtomicUsize,
}

#[allow(clippy::non_send_fields_in_send_ty)]
unsafe impl<T: Send> Send for Arena<T> {}
unsafe impl<T: Send + Sync> Sync for Arena<T> {}

// NOTE: can make drop much faster if we use the Arena's properties.

impl<T> Drop for Arena<T> {
    fn drop(&mut self) {
        debug_assert_eq!(*self.index.get_mut(), *self.count.get_mut());

        for (i, bucket) in self.buckets.iter_mut().enumerate() {
            // SAFETY: Arena.buckets is sound
            unsafe { bucket.try_dealloc(i) };
        }
    }
}

impl<T> Arena<T> {
    #[expect(clippy::declare_interior_mutable_const)]
    const EMPTY: Self = Self {
        buckets: [Bucket::EMPTY; BUCKETS],
        index: AtomicUsize::new(0),
        count: AtomicUsize::new(0),
    };

    /// Construct a new, empty, arena
    pub const fn new() -> Self {
        Self::EMPTY
    }

    pub fn with_capacity(capacity: usize) -> Self {
        // SAFETY: capacity is bounded to MIN_INDEX
        let loc = unsafe { Location::new_unchecked(capacity.min(MAX_INDEX)) };

        let mut arena = Self::new();
        for (i, bucket) in arena.buckets[..=loc.bucket].iter_mut().enumerate() {
            // SAFETY: bucket is uninit, capacity is based on i, which is correct
            unsafe { bucket.overwrite(Location::capacity(i)) };
        }
        arena
    }

    /// Get a node at index
    pub fn get(&self, index: Index) -> Option<&Node<T>> {
        // SAFETY: using loc.bucket & loc.entry always results in sound indexing
        let loc = Location::new(index);
        unsafe { self.bucket_at(loc).get(loc.entry) }?.get()
    }

    /// Returns a unique index for insertion.
    fn next_index(&self) -> Index {
        if let index @ ..=MAX_INDEX = self.index.fetch_add(1, Relaxed) {
            // SAFETY: checked above
            unsafe { Index::new_unchecked(index) }
        } else {
            self.index.fetch_sub(1, Relaxed);
            panic!("capacity overflow");
        }
    }

    pub fn push_with(&self, parent: Option<&Node<T>>, f: impl FnOnce(Index) -> T) -> &Node<T> {
        let index = self.next_index();
        // SAFETY: Index is unique
        unsafe { self.add_node(parent, index, f(index)) }
    }

    pub fn push_all(
        &self,
        parent: Option<&Node<T>>,
        values: impl ExactSizeIterator<Item = T>,
    ) -> impl ExactSizeIterator<Item = &Node<T>> {
        let origin = self
            .index
            .fetch_update(Relaxed, Relaxed, |index| {
                index.checked_add(values.len()).filter(|&n| n <= MAX_INDEX)
            })
            .expect("capacity overflow");
        let len = values.len();

        values.enumerate().map(move |(i, value)| {
            assert!(i < len, "iterator returned extra value");
            // SAFETY: index is unique & checked above
            unsafe { self.add_node(parent, Index::new_unchecked(origin + i), value) }
        })
    }

    /// add a new node
    ///
    /// # Safety
    ///
    /// Index must be unique, `parent` must be from this arena
    #[inline]
    unsafe fn add_node(&self, parent: Option<&Node<T>>, index: Index, value: T) -> &Node<T> {
        let loc = Location::new(index);
        // SAFETY: index is unique
        let node = unsafe {
            self.bucket_at(loc)
                .acquire(loc)
                .write(Node::new(index, parent, value), parent)
        };

        self.count.fetch_add(1, Relaxed);
        node
    }

    pub fn reserve(&self, additional: usize) {
        let index = self
            .count
            .load(Acquire)
            .saturating_add(additional)
            .min(MAX_INDEX);
        // SAFETY: index checked above
        let mut loc = unsafe { Location::new_unchecked(index) };
        while !self.bucket_at(loc).is_alloc() {
            // SAFETY: same index used = same bucket
            unsafe { self.bucket_at(loc).reserve(loc.bucket) };
            if loc.bucket == 0 {
                break;
            }
            loc.bucket -= 1;
        }
    }

    pub fn capacity(&self) -> usize {
        let mut total = 0;
        for bucket in 0..BUCKETS {
            if self.buckets[bucket].is_alloc() {
                total += Location::capacity(bucket);
            }
        }
        total
    }

    pub fn contains(&self, node: &Node<T>) -> bool {
        self.get(node.index())
            .is_some_and(|found| ptr::eq(found, node))
    }

    pub fn count(&self) -> usize {
        self.count.load(Relaxed)
    }

    /// Get the bucket at the given `Location`
    ///
    /// This is safe since `Location.bucket` is always within bounds
    #[inline]
    fn bucket_at(&self, Location { bucket, .. }: Location) -> &Bucket<Slot<T>> {
        // SAFETY: Location.bucket is always within bounds
        unsafe { self.buckets.get_unchecked(bucket) }
    }
}

/// A valid(possibly uninit) location within the arena
#[derive(Debug, Clone, Copy)]
pub struct Location {
    /// the bucket
    pub bucket: usize,
    /// a slot within the bucket
    pub entry: usize,
}

impl Location {
    /// Create a new location without checking
    ///
    /// # Safety
    ///
    /// `index` <= [`MAX_INDEX`]
    #[inline]
    pub const unsafe fn new_unchecked(index: usize) -> Self {
        debug_assert!(index <= MAX_INDEX);
        let index = index + ZERO_SLOT;
        let bucket = Self::bucket(index);
        let entry = index - (Self::capacity(bucket) - 1);
        Self { bucket, entry }
    }

    #[inline]
    pub const fn new(index: Index) -> Self {
        // SAFETY: Index is always <= MAX_INDEX
        unsafe { Self::new_unchecked(index.get()) }
    }

    /// The bucket `index` - [`ZERO_SLOT`] belongs to
    #[inline]
    pub const fn bucket(index: usize) -> usize {
        BUCKETS - (index + 1).leading_zeros() as usize
    }

    /// The number of slots in the given bucket
    #[inline]
    pub const fn capacity(bucket: usize) -> usize {
        1 << (bucket + ZERO_BUCKET)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    impl From<usize> for Location {
        fn from(index: usize) -> Self {
            assert!(index <= MAX_INDEX, "index out of bounds");
            // SAFETY: index checked abvoe
            Location::new(unsafe { Index::new_unchecked(index) })
        }
    }

    #[test]
    fn location() {
        assert_eq!(Location::capacity(0), SLOTS);

        for i in 0..SLOTS {
            let loc = Location::from(i);
            assert_eq!(loc.bucket, 0);
            assert_eq!(loc.entry, i);
        }

        assert_eq!(Location::capacity(1), SLOTS * 2);

        for i in SLOTS..SLOTS * 3 {
            let loc = Location::from(i);
            assert_eq!(loc.bucket, 1);
            assert_eq!(loc.entry, i - SLOTS);
        }

        assert_eq!(Location::capacity(2), SLOTS * 4);

        for i in SLOTS * 3..SLOTS * 7 {
            let loc = Location::from(i);
            assert_eq!(loc.bucket, 2);
            assert_eq!(loc.entry, i - SLOTS * 3);
        }
    }

    #[test]
    fn max_entries() {
        let mut slots = 0;
        for i in 0..BUCKETS {
            slots += Location::capacity(i);
        }

        assert_eq!(slots, MAX_INDEX + 1);

        let max = Location::from(MAX_INDEX);
        assert_eq!(max.bucket, BUCKETS - 1);
        assert_eq!(max.entry, (1 << (usize::BITS - 2)) - 1);
    }
}
