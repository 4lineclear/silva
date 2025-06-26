use std::ptr;
use std::sync::atomic::AtomicPtr;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering::{Acquire, Relaxed};

use crate::node::Node;

use super::Index;
use super::slot::Chunk;

/// The total number of slots
pub const SLOTS: usize = (usize::BITS / 2) as usize;
/// The number of skipped slots
pub const ZERO_SLOT: usize = SLOTS - 1;
/// The number of skipped buckets
pub const ZERO_BUCKET: usize = (usize::BITS - ZERO_SLOT.leading_zeros()) as usize;
/// The number of buckets to be used
pub const BUCKETS: usize = usize::BITS as usize - 1 - ZERO_BUCKET;
/// The inclusive max index(slot) able to be stored
pub const MAX_INDEX: usize = isize::MAX as usize - ZERO_SLOT - 1;

pub struct Arena<T> {
    buckets: [AtomicPtr<Chunk<T>>; BUCKETS],
    index: AtomicUsize,
    count: AtomicUsize,
}

// TODO: consider lowering requirements here
unsafe impl<T: Send + Sync> Send for Arena<T> {}
unsafe impl<T: Send + Sync> Sync for Arena<T> {}

impl<T> std::ops::Index<Index> for Arena<T> {
    type Output = Node<T>;

    #[inline]
    fn index(&self, index: Index) -> &Self::Output {
        self.get(index).expect("index is uninitialized")
    }
}

impl<T> Drop for Arena<T> {
    fn drop(&mut self) {
        for (i, bucket) in self.buckets.iter_mut().enumerate() {
            let chunks = *bucket.get_mut();
            if !chunks.is_null() {
                let len = Location::chunk_cap(i);
                unsafe { Chunk::dealloc(chunks, len) };
            }
        }
    }
}

impl<T> Default for Arena<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Arena<T> {
    pub fn count(&self) -> usize {
        self.count.load(Relaxed)
    }

    /// Construct a new, empty, `Buckets`.
    pub const fn new() -> Self {
        Self {
            buckets: [const { AtomicPtr::new(ptr::null_mut()) }; BUCKETS],
            index: AtomicUsize::new(0),
            count: AtomicUsize::new(0),
        }
    }

    pub fn get(&self, index: Index) -> Option<&Node<T>> {
        let loc = Location::new(index);
        unsafe { (*self.get_chunk(loc)?).get(loc.slot) }
    }

    fn get_chunk(&self, loc: Location) -> Option<&Chunk<T>> {
        let bucket = unsafe { self.buckets.get_unchecked(loc.bucket) }.load(Acquire);
        (!bucket.is_null()).then(|| unsafe { &*bucket.add(loc.chunk) })
    }

    /// Returns a unique index for insertion.
    fn next_index(&self) -> Index {
        let index = self.index.fetch_add(1, Relaxed);
        if index > MAX_INDEX {
            self.index.fetch_sub(1, Relaxed);
            panic!("capacity overflow");
        }
        // SAFETY: checked above
        unsafe { Index::new_unchecked(index) }
    }

    pub fn push_with(&self, parent: Option<&Node<T>>, f: impl FnOnce(Index) -> T) -> &Node<T> {
        let index = self.next_index();
        let loc = Location::new(index);

        let node = Node::new(index, parent.map(Node::index), f(index));
        let node = if let Some(parent) = parent {
            unsafe { (*self.chunk(loc)).write(loc.slot, node, index, parent) }
        } else {
            unsafe { (*self.chunk(loc)).write_root(loc.slot, node) }
        };

        self.count.fetch_add(1, Relaxed);
        node
    }

    unsafe fn chunk(&self, loc: Location) -> &Chunk<T> {
        let bucket = unsafe { self.buckets.get_unchecked(loc.bucket) };
        let mut chunks = bucket.load(Acquire);

        if chunks.is_null() {
            let len = Location::chunk_cap(loc.bucket);
            chunks = unsafe { Chunk::alloc_bucket(bucket, len) };
        }
        unsafe { &*chunks.add(loc.chunk) }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        let loc = Location::new(unsafe { Index::new_unchecked(capacity.min(MAX_INDEX)) });

        let mut arena = Self::new();
        for (i, bucket) in arena.buckets[..=loc.bucket].iter_mut().enumerate() {
            let len = Location::chunk_cap(i);
            *bucket = AtomicPtr::new(unsafe { Chunk::alloc_bucket(bucket, len) });
        }
        arena
    }

    pub fn reserve(&self, additional: usize) {
        let index = self
            .count
            .load(Acquire)
            .saturating_add(additional)
            .min(MAX_INDEX);
        // SAFETY: index checked above
        let index = unsafe { Index::new_unchecked(index) };
        let mut loc = Location::new(index);
        loop {
            let bucket = unsafe { self.buckets.get_unchecked(loc.bucket) };
            let chunks = bucket.load(Acquire);
            if !chunks.is_null() {
                break;
            }
            let len = Location::chunk_cap(loc.bucket);
            unsafe { Chunk::alloc_bucket(bucket, len) };
            if loc.bucket == 0 {
                break;
            }
            loc.bucket -= 1;
        }
    }

    pub fn capacity(&self) -> usize {
        let mut total = 0;
        for bucket in 0..BUCKETS {
            if !self.buckets[bucket].load(Relaxed).is_null() {
                total += Location::slot_cap(bucket);
            }
        }
        total
    }
}

/// A valid(possibly uninit) location within the arena
#[derive(Debug, Clone, Copy)]
pub struct Location {
    /// the bucket
    bucket: usize,
    /// the specific chunk within the bucket
    chunk: usize,
    /// the specific slot within the chunk
    slot: usize,
}

impl Location {
    #[inline]
    pub const fn new(index: Index) -> Self {
        let index = index.get() + ZERO_SLOT;
        let bucket = BUCKETS - (index + 1).leading_zeros() as usize;
        let entry = index - (Self::slot_cap(bucket) - 1);
        Self {
            bucket,
            chunk: entry / SLOTS,
            slot: entry % SLOTS,
        }
    }

    /// The number of chunks in the given bucket
    #[inline]
    pub const fn chunk_cap(bucket: usize) -> usize {
        1 << bucket
    }

    /// The number of slots in the given bucket
    #[inline]
    pub const fn slot_cap(bucket: usize) -> usize {
        1 << (bucket + ZERO_BUCKET)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    impl Location {
        fn from_usize(index: usize) -> Self {
            assert!(index <= MAX_INDEX, "index out of bounds");
            Location::new(unsafe { Index::new_unchecked(index) })
        }
    }

    #[test]
    fn location() {
        assert_eq!(Location::chunk_cap(0), 1);

        for i in 0..SLOTS {
            let loc = Location::from_usize(i);
            assert_eq!(loc.bucket, 0);
            assert_eq!(loc.chunk, i / SLOTS);
            assert_eq!(loc.slot, i);
        }

        assert_eq!(Location::chunk_cap(1), 2);

        for i in SLOTS..SLOTS * 3 {
            let loc = Location::from_usize(i);
            assert_eq!(loc.bucket, 1);
            assert_eq!(loc.chunk, (i - SLOTS) / SLOTS);
            assert_eq!(loc.slot, i % SLOTS);
        }

        assert_eq!(Location::chunk_cap(2), 4);

        for i in SLOTS * 3..SLOTS * 7 {
            let loc = Location::from_usize(i);
            assert_eq!(loc.bucket, 2);
            assert_eq!(loc.chunk, (i - SLOTS * 3) / SLOTS);
            assert_eq!(loc.slot, i % SLOTS);
        }
    }

    #[test]
    fn max_entries() {
        /// the number of chunks for the biggest bucket
        pub const MAX_CHUNK: usize = isize::MAX as usize / usize::BITS as usize;

        let mut chunks = 0;
        let mut slots = 0;
        for i in 0..BUCKETS {
            slots += Location::slot_cap(i);
            chunks += Location::chunk_cap(i);
        }
        assert_eq!(slots, MAX_INDEX + 1);
        assert_eq!(chunks, MAX_CHUNK * 2 + 1);

        let max = Location::from_usize(MAX_INDEX);
        assert_eq!(max.bucket, BUCKETS - 1);
        assert_eq!(Location::chunk_cap(max.bucket), MAX_CHUNK + 1);
        assert_eq!(max.chunk, MAX_CHUNK);
        assert_eq!(max.slot, SLOTS - 1);
    }
}
