//! The nodes within an arena

use std::ptr::{self, NonNull};
use std::sync::Arc;
use std::sync::atomic::AtomicPtr;
use std::sync::atomic::Ordering::Acquire;

use crate::{Arena, Index};

// NOTE: moving to pointers sets off miri, resulting in stacked-borrow related errors

/// A node within an arena
#[derive(Debug)]
pub struct Node<T> {
    /// This node's index, added for convenience
    pub(crate) index: Index,
    /// This nodes's parent
    pub(crate) parent: Option<NonNull<Self>>,
    /// This nodes's last added child
    pub(crate) child: AtomicPtr<Self>,
    /// The node after this one
    pub(crate) next: Option<NonNull<Self>>,
    /// The node's data
    pub value: T,
}

unsafe impl<T: Send> Send for Node<T> {}
unsafe impl<T: Send + Sync> Sync for Node<T> {}

impl<T> Node<T> {
    /// create a new node
    ///
    /// # Safety
    ///
    /// The given `parent` should be located in the arena this node is to put in.
    pub(crate) unsafe fn new(index: Index, parent: Option<&Self>, value: T) -> Self {
        Self {
            index,
            parent: parent.map(NonNull::from),
            child: AtomicPtr::new(ptr::null_mut()),
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
    pub fn parent(&self) -> Option<&Self> {
        unsafe { self.parent.map(|n| n.as_ref()) }
    }

    /// Get this node's latest child
    ///
    /// If [`None`] this node is a leaf
    pub fn child(&self) -> Option<&Self> {
        unsafe { self.child.load(Acquire).as_ref() }
    }

    /// Get this node's next sibling
    pub fn next(&self) -> Option<&Self> {
        unsafe { self.next.map(|n| n.as_ref()) }
    }

    /// Iterate over the ancestors of this node
    ///
    /// Iterator starts from this node's parent
    pub fn ancestors(&self) -> Ancestors<'_, T> {
        Ancestors {
            curr: self.parent(),
        }
    }

    /// Iterate over the children of this node
    pub fn children(&self) -> Next<'_, T> {
        Next { curr: self.child() }
    }

    /// Iterate over the next(previously added) nodes
    ///
    /// Skips this node
    pub fn iter_next(&self) -> Next<'_, T> {
        Next { curr: self.next() }
    }
}

/// Iterates over nodes using [`Node::next`]
pub struct Next<'a, T> {
    curr: Option<&'a Node<T>>,
}

impl<'a, T> Iterator for Next<'a, T> {
    type Item = &'a Node<T>;

    fn next(&mut self) -> Option<Self::Item> {
        let node = self.curr.take()?;
        self.curr = node.next();
        Some(node)
    }
}

/// Iterates over nodes using [`Node::parent`]
pub struct Ancestors<'a, T> {
    curr: Option<&'a Node<T>>,
}

impl<'a, T> Iterator for Ancestors<'a, T> {
    type Item = &'a Node<T>;

    fn next(&mut self) -> Option<Self::Item> {
        let node = self.curr.take()?;
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
