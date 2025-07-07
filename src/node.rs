//! The nodes within an arena

use std::ptr::NonNull;
use std::sync::Arc;

use crate::{Arena, AtomicIndex, Index};

// NOTE: moving to pointers sets off miri, resulting in stacked-borrow related errors

/// A node within an arena
#[derive(Debug)]
pub struct Node<T> {
    /// This node's index, added for convenience
    pub(crate) index: Index,
    /// This nodes's parent
    pub(crate) parent: Option<Index>,
    /// This nodes's last added child
    pub(crate) child: AtomicIndex,
    /// The node after this one
    pub(crate) next: Option<Index>,
    /// The node's data
    pub value: T,
}

impl<T> Node<T> {
    /// create a new node
    ///
    /// # Safety
    ///
    /// The given `parent` should be located in the arena this node is to put in.
    pub(crate) const unsafe fn new(index: Index, parent: Option<Index>, value: T) -> Self {
        Self {
            index,
            parent,
            child: AtomicIndex::none(),
            next: None,
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
    #[expect(clippy::missing_const_for_fn)]
    pub fn parent(&self) -> Option<Index> {
        self.parent
    }

    /// Get this node's latest child
    ///
    /// If [`None`] this node is a leaf
    pub fn child(&self) -> Option<Index> {
        self.child.get()
    }

    /// Get this node's next sibling
    #[expect(clippy::missing_const_for_fn)]
    pub fn next(&self) -> Option<Index> {
        self.next
    }

    /// Iterate over the ancestors of this node
    ///
    /// Iterator starts from this node's parent
    ///
    /// # Panics
    ///
    /// This function will panic if this `node` isn't held within this `arena`
    pub fn ancestors<'a>(&'a self, arena: &'a Arena<T>) -> Ancestors<'a, T> {
        assert!(arena.contains(self), "this node does not belong to arena");
        Ancestors {
            curr: self.parent(),
            arena,
        }
    }

    /// Iterate over the children of this node
    ///
    /// # Panics
    ///
    /// This function will panic if this `node` isn't held within this `arena`
    ///
    pub fn children<'a>(&'a self, arena: &'a Arena<T>) -> Next<'a, T> {
        assert!(arena.contains(self), "this node does not belong to arena");
        Next {
            curr: self.child(),
            arena,
        }
    }

    /// Iterate over the next(previously added) nodes
    ///
    /// Skips this node
    ///
    /// # Panics
    ///
    /// This function will panic if this `node` isn't held within this `arena`
    ///
    pub fn iter_next<'a>(&'a self, arena: &'a Arena<T>) -> Next<'a, T> {
        assert!(arena.contains(self), "this node does not belong to arena");
        Next {
            curr: self.next(),
            arena,
        }
    }
}

/// Iterates over nodes using [`Node::next`]
pub struct Next<'a, T> {
    curr: Option<Index>,
    arena: &'a Arena<T>,
}

impl<'a, T> Iterator for Next<'a, T> {
    type Item = &'a Node<T>;

    fn next(&mut self) -> Option<Self::Item> {
        // SAFETY: these node indices are always valid
        let node = unsafe { self.arena.get_unchecked(self.curr.take()?) };
        self.curr = node.next();
        Some(node)
    }
}

/// Iterates over nodes using [`Node::parent`]
pub struct Ancestors<'a, T> {
    curr: Option<Index>,
    arena: &'a Arena<T>,
}

impl<'a, T> Iterator for Ancestors<'a, T> {
    type Item = &'a Node<T>;

    fn next(&mut self) -> Option<Self::Item> {
        // SAFETY: these node indices are always valid
        let node = unsafe { self.arena.get_unchecked(self.curr.take()?) };
        self.curr = node.parent();
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
        // SAFETY: when correctly created(according to Handle::new)
        // this should always be valid
        unsafe { self.node.as_ref() }
    }
}
