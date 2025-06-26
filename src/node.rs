//! The nodes within an arena

use std::ptr::NonNull;
use std::sync::Arc;

use crate::arena::{Arena, AtomicIndex, Index};

// TODO: create way to allocate many siblings at once

// TODO: consider moving locking to AtomicIndex,
// in which case node removal can be added. also slots can be widened to usize::BITS

// TODO: consider creating version that uses AtomicPtr<Node<T>> or AtomicPtr<Slot<T>>
// for PNode the reserved 'index' could be the ptr to the current node.

/// A node within an arena
#[derive(Debug)]
pub struct Node<T> {
    /// This node's index, added for convenience
    pub(crate) index: Index,
    /// This nodes's parent
    pub(crate) parent: AtomicIndex,
    /// This nodes's first and latest added child
    pub(crate) child: AtomicIndex,
    /// The node after this one
    pub(crate) next: AtomicIndex,
    /// The node's data
    pub value: T,
}

impl<T> Node<T> {
    pub(crate) const fn new(index: Index, parent: Option<Index>, value: T) -> Self {
        Self {
            index,
            parent: AtomicIndex::opt(parent),
            child: AtomicIndex::none(),
            next: AtomicIndex::none(),
            value,
        }
    }

    /// Get this node's index
    pub const fn index(&self) -> Index {
        self.index
    }

    /// Get this node's parent
    ///
    /// If [`None`] this node is a root
    pub fn parent(&self) -> Option<Index> {
        self.parent.get()
    }

    /// Get this node's latest child
    ///
    /// If [`None`] this node is a leaf
    pub fn child(&self) -> Option<Index> {
        self.child.get()
    }

    /// Get this node's next sibling
    pub fn next(&self) -> Option<Index> {
        self.next.get()
    }

    /// Iterate over the ancestors of this node
    ///
    /// Iterator starts from this node's parent
    pub fn ancestors<'a>(&self, arena: &'a Arena<T>) -> Ancestors<'a, T> {
        Ancestors {
            curr: arena.parent(self),
            arena,
        }
    }

    /// Iterate over the children of this node
    pub fn children<'a>(&self, arena: &'a Arena<T>) -> Next<'a, T> {
        Next {
            curr: arena.child(self),
            arena,
        }
    }

    /// Iterate over the next(previously added) nodes
    ///
    /// Skips this node
    pub fn iter<'a>(&self, arena: &'a Arena<T>) -> Next<'a, T> {
        Next {
            curr: arena.next(self),
            arena,
        }
    }

    /// returns true if this is the first child of it's parent
    pub fn is_first(&self, arena: &Arena<T>) -> bool {
        arena
            .parent(self)
            .is_some_and(|parent| parent.child() == Some(self.index))
    }

    /// returns `true` if this node has no parent
    pub fn is_root(&self) -> bool {
        self.next().is_some()
    }

    /// returns `true` if this node has parent and a child
    pub fn is_branch(&self) -> bool {
        !self.is_root() && !self.is_leaf()
    }

    /// returns `true` if this does not have a child
    pub fn is_leaf(&self) -> bool {
        self.child().is_none()
    }
}

/// Iterates over nodes using [`Node::next`]
pub struct Next<'a, T> {
    curr: Option<&'a Node<T>>,
    arena: &'a Arena<T>,
}

impl<'a, T> Iterator for Next<'a, T> {
    type Item = &'a Node<T>;

    fn next(&mut self) -> Option<Self::Item> {
        let node = self.curr.take()?;
        self.curr = self.arena.next(node);
        Some(node)
    }
}

/// Iterates over nodes using [`Node::parent`]
pub struct Ancestors<'a, T> {
    curr: Option<&'a Node<T>>,
    arena: &'a Arena<T>,
}

impl<'a, T> Iterator for Ancestors<'a, T> {
    type Item = &'a Node<T>;

    fn next(&mut self) -> Option<Self::Item> {
        let node = self.curr.take()?;
        self.curr = self.arena.parent(node);
        Some(node)
    }
}

/// A non-owning handle to a node
///
/// Uses an [`Arc`] to an [`Arena`] to safely forego a lifetime
pub struct Handle<T> {
    node: NonNull<Node<T>>,
    arena: Arc<Arena<T>>,
}

unsafe impl<T: Send + Sync> Send for Handle<T> {}
unsafe impl<T: Send + Sync> Sync for Handle<T> {}

impl<T> Handle<T> {
    /// create a new handle
    ///
    /// # Safety
    ///
    /// The given node should be obtained from the given [`Arena`]
    pub(crate) unsafe fn new(node: &Node<T>, arena: &Arc<Arena<T>>) -> Self {
        Self {
            node: NonNull::from(node),
            arena: arena.clone(),
        }
    }

    /// Get the underlying arena
    pub const fn arena(&self) -> &Arc<Arena<T>> {
        &self.arena
    }
}

impl<T: std::ops::Deref> std::ops::Deref for Handle<T> {
    type Target = Node<T>;

    fn deref(&self) -> &Self::Target {
        // SAFETY: when correctly create(according to Handle::new)
        // this should always be valid
        unsafe { self.node.as_ref() }
    }
}
