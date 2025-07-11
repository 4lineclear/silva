//! the arena's implementation
//!
//! # Implementation
//!
//! Based on [boxcar], [slotmap-boxcar], and [sharded-slab]
//!
//! [boxcar]: https://github.com/ibraheemdev/boxcar
//! [slotmap-boxcar]: https://github.com/SabrinaJewson/boxcar.rs
//! [sharded-slab]: https://github.com/hawkw/sharded-slab

use crate::{AsParent, Handle, Index, Node};

use std::sync::Arc;

// NOTE: should move bucket & slot to be submodules of raw

mod bucket;
mod raw;
mod slot;

// export just for Index
pub use raw::MAX_INDEX;

// TODO: create way to allocate many siblings at once

/// The arena where [`Node`]s are stored
pub struct Arena<T> {
    raw: raw::Arena<T>,
}

impl<T> Default for Arena<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> std::ops::Index<Index> for Arena<T> {
    type Output = Node<T>;

    #[inline]
    fn index(&self, index: Index) -> &Self::Output {
        self.get(index).expect("index is uninitialized")
    }
}

impl<T> Arena<T> {
    /// Construct a new, empty, tree.
    pub const fn new() -> Self {
        Self {
            raw: raw::Arena::new(),
        }
    }

    /// Create a tree with atleast the given capacity
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            raw: raw::Arena::with_capacity(capacity),
        }
    }

    /// Reserve atleast `additional` more slots
    pub fn reserve(&self, additional: usize) {
        self.raw.reserve(additional);
    }

    /// Get the node of the given [`Index`]
    ///
    /// This returns an option since `index` may have come from another [`Arena`]
    pub fn get(&self, index: Index) -> Option<&Node<T>> {
        self.raw.get(index)
    }

    /// Get a handle for the node of the given [`Index`]
    pub fn get_handle(self: &Arc<Self>, index: impl Into<Index>) -> Option<Handle<T>> {
        // SAFETY: node is obtained from correct arena
        Some(unsafe { Handle::new(self.raw.get(index.into())?, self) })
    }

    /// Add a new node
    pub fn push(&self, parent: impl AsParent<T>, value: T) -> &Node<T> {
        self.raw.push_with(parent.get(self), |_| value)
    }

    /// Add a new node using the given function
    pub fn push_with(&self, parent: impl AsParent<T>, f: impl FnOnce(Index) -> T) -> &Node<T> {
        self.raw.push_with(parent.get(self), f)
    }

    /// Add new nodes using the given iterator
    pub fn push_all(
        &self,
        parent: impl AsParent<T>,
        values: impl IntoIterator<Item = T, IntoIter: ExactSizeIterator>,
    ) -> impl ExactSizeIterator<Item = &Node<T>> {
        self.raw.push_all(parent.get(self), values.into_iter())
    }

    /// Get a handle to an index
    ///
    /// # Panics
    ///
    /// Panics if the index does not exist within this arena
    pub fn handle(self: &Arc<Self>, index: impl Into<Index>) -> Handle<T> {
        // SAFETY: node is obtained from correct arena
        unsafe { Handle::new(&self[index.into()], self) }
    }

    /// returns `true` if the given node belongs to this arena
    pub fn contains(&self, node: &Node<T>) -> bool {
        self.raw.contains(node)
    }

    /// Get the number of available nodes
    pub fn count(&self) -> usize {
        self.raw.count()
    }

    /// Get the number of available slots
    ///
    /// `capacity` + `SLOTS`([`usize::BITS`]) should always be a power of two.
    pub fn capacity(&self) -> usize {
        self.raw.capacity()
    }
}
