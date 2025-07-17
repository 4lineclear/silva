//! the arena's implementation
//!
//! # Implementation
//!
//! Based on [boxcar], [slotmap-boxcar], and [sharded-slab]
//!
//! [boxcar]: https://github.com/ibraheemdev/boxcar
//! [slotmap-boxcar]: https://github.com/SabrinaJewson/boxcar.rs
//! [sharded-slab]: https://github.com/hawkw/sharded-slab

use crate::{AsParent, Index, Node};

// NOTE: should move bucket & slot to be submodules of raw

mod bucket;
mod raw;
mod slot;

// export just for Index
pub use raw::MAX_INDEX;

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

// struct IterNodes<'a, T> {
//     pos: usize,
//     arena: &'a Arena<T>,
// }
//
// impl<'a, T> IterNodes<'a, T> {
//     fn new(arena: &'a Arena<T>) -> Self {
//         let pos = 0;
//         Self { pos, arena }
//     }
// }
//
// struct MaybeNode<'a, T>(Option<&'a Node<T>>);
//
// impl<'a, T: fmt::Debug> fmt::Debug for MaybeNode<'a, T> {
//     fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
//         if let Some(node) = self.0 {
//             node.fmt(f)
//         } else {
//             f.write_str("null")
//         }
//     }
// }
//
// impl<'a, T> Iterator for IterNodes<'a, T> {
//     type Item = MaybeNode<'a, T>;
//
//     fn next(&mut self) -> Option<Self::Item> {
//         if self.pos >= self.arena.count() {
//             return None;
//         }
//
//         let node = self.arena.get(unsafe { Index::new_unchecked(self.pos) });
//         self.pos += 1;
//         Some(MaybeNode(node))
//     }
// }
//
// impl<T: fmt::Debug> fmt::Debug for Arena<T> {
//     fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
//         f.debug_list().entries(IterNodes::new(self)).finish()
//     }
// }
