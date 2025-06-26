//! an underlying arena
//!
//! # Implementation
//!
//! Based on [boxcar], [slotmap-boxcar], and [sharded-slab]
//!
//! [boxcar]: https://github.com/ibraheemdev/boxcar
//! [slotmap-boxcar]: https://github.com/SabrinaJewson/boxcar.rs
//! [sharded-slab]: https://github.com/hawkw/sharded-slab

use crate::node::{Handle, Node};

use std::fmt::Display;
use std::num::NonZero;
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering::Acquire;

mod raw;
mod slot;

/// The underlying arena.
///
/// This structure holds trees & their nodes, providing multithreaded access to them
/// when using an [`Index`].
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
        &self.raw[index]
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
    pub fn get_handle(self: &Arc<Self>, index: Index) -> Option<Handle<T>> {
        // SAFETY: node is obtained from correct arena
        Some(unsafe { Handle::new(self.raw.get(index)?, self) })
    }

    /// Add a new node
    pub fn push(&self, parent: Option<&Node<T>>, value: T) -> &Node<T> {
        self.raw.push_with(parent, |_| value)
    }

    /// Add a new node using the given function
    pub fn push_with(&self, parent: Option<&Node<T>>, f: impl FnOnce(Index) -> T) -> &Node<T> {
        self.raw.push_with(parent, f)
    }

    /// Add a new node to an existing one
    pub fn handle(self: &Arc<Self>, parent: Option<&Node<T>>, value: T) -> Handle<T> {
        // SAFETY: node is obtained from correct arena
        unsafe { Handle::new(self.raw.push_with(parent, |_| value), self) }
    }

    /// Add a new node to an existing one using the given function
    pub fn handle_with(
        self: &Arc<Self>,
        parent: Option<&Node<T>>,
        f: impl FnOnce(Index) -> T,
    ) -> Handle<T> {
        // SAFETY: node is obtained from correct arena
        unsafe { Handle::new(self.raw.push_with(parent, f), self) }
    }

    /// Get the node at [`Node::parent`]
    pub fn parent(&self, node: &Node<T>) -> Option<&Node<T>> {
        self.get(node.parent()?)
    }

    /// Get the node at [`Node::child`]
    pub fn child(&self, node: &Node<T>) -> Option<&Node<T>> {
        self.get(node.child()?)
    }

    /// Get the node at [`Node::next`]
    pub fn next(&self, node: &Node<T>) -> Option<&Node<T>> {
        self.get(node.next()?)
    }

    /// Get the number of available nodes
    pub fn count(&self) -> usize {
        self.raw.count()
    }

    /// Get the number of available slots
    pub fn capacity(&self) -> usize {
        self.raw.capacity()
    }
}

/// A valid index into an arena
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Index(NonZero<usize>);

impl std::fmt::Debug for Index {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Index").field(&self.get()).finish()
    }
}

impl Display for Index {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.get().fmt(f)
    }
}

impl Index {
    /// creates new index
    ///
    /// # Safety
    ///
    /// index must be less than or equal to `MAX_INDEX`
    const unsafe fn new_unchecked(index: usize) -> Self {
        debug_assert!(index <= raw::MAX_INDEX);
        Self(unsafe { NonZero::new_unchecked(index + 1) })
    }

    /// returns the index this arena is stored at
    pub(crate) const fn get(self) -> usize {
        self.0.get() - 1
    }
}

/// An optional atomic [`Index`]
pub struct AtomicIndex(AtomicUsize);

impl std::fmt::Debug for AtomicIndex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.get().fmt(f)
    }
}

impl AtomicIndex {
    pub fn get(&self) -> Option<Index> {
        NonZero::new(self.0.load(Acquire)).map(Index)
    }

    pub(crate) const fn new(index: Index) -> Self {
        Self(AtomicUsize::new(index.0.get()))
    }

    pub(crate) const fn opt(index: Option<Index>) -> Self {
        match index {
            Some(t) => Self::new(t),
            None => Self::none(),
        }
    }

    pub(crate) const fn none() -> Self {
        Self(AtomicUsize::new(0))
    }
}
