//! The nodes within an arena

use std::fmt::Debug;
use std::ptr::{self, NonNull};
use std::sync::Arc;
use std::sync::atomic::AtomicPtr;

use crate::{Arena, Index};
use std::sync::atomic::Ordering::AcqRel;
use std::sync::atomic::Ordering::Acquire;

/// A node within an arena
#[derive(Debug)]
pub struct Node<T> {
    /// This node's index, added for convenience
    index: Index,
    /// This nodes's parent
    parent: *const Self,
    /// This nodes's last added child
    child: AtomicPtr<Self>,
    /// The node after this one
    next: *const Self,
    /// The node's data
    pub value: T,
}

const fn map_ref<T>(v: Option<&T>) -> *const T {
    if let Some(v) = v {
        ptr::from_ref(v)
    } else {
        ptr::null()
    }
}

impl<T> Node<T> {
    /// create a new node
    ///
    /// # Safety
    ///
    /// The given `parent` should be located in the arena this node is to put in.
    pub(crate) const unsafe fn new(index: Index, parent: Option<&Self>, value: T) -> Self {
        Self {
            index,
            parent: map_ref(parent),
            child: AtomicPtr::new(ptr::null_mut()),
            next: ptr::null_mut(),
            value,
        }
    }

    /// turns this node into a handle using the given arena
    ///
    /// # Panics
    ///
    /// panics if this `node` is not from the given `arena`
    pub fn handle(&self, arena: &Arc<crate::Arena<T>>) -> Handle<T> {
        if arena.contains(self) {
            // SAFETY: node is confirmed to be in the arena
            unsafe { Handle::new(self, arena) }
        } else {
            panic!("this node does not belong to the given arena")
        }
    }

    /// A convenient wrapper for debugging a node
    pub const fn debug(&self) -> DebugNode<T> {
        DebugNode(self)
    }

    /// Get this node's index
    pub const fn index(&self) -> Index {
        self.index
    }

    /// Get this node's parent
    ///
    /// If [`None`] this node is a root
    #[expect(clippy::missing_const_for_fn)]
    pub fn parent(&self) -> Option<&Self> {
        // SAFETY: Node.parent is always sound
        unsafe { self.parent.as_ref() }
    }

    /// Get this node's latest child
    ///
    /// If [`None`] this node is a leaf
    pub fn child(&self) -> Option<&Self> {
        // SAFETY: Node.child is always sound
        unsafe { self.child.load(Acquire).as_ref() }
    }

    /// Get this node's next sibling
    #[expect(clippy::missing_const_for_fn)]
    pub fn next(&self) -> Option<&Self> {
        // SAFETY: Node.next is always sound
        unsafe { self.next.as_ref() }
    }

    /// Add a child to this node
    ///
    /// # Safety
    ///
    /// The given `node` must belong to the same arena as this one. The ptr to
    /// the `node` must be valid.
    pub(crate) unsafe fn add_child(&self, child: *mut Self) {
        // SAFETY: upheld by caller
        debug_assert!(unsafe { ptr::eq((*child).parent, self) });

        let mut prev = self.child.load(Acquire);
        loop {
            // SAFETY: upheld by caller
            unsafe { (*child).next = prev };

            match self
                .child
                .compare_exchange_weak(prev, child, AcqRel, Acquire)
            {
                Err(next_prev) => prev = next_prev,
                Ok(_) => break,
            }
        }
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
#[derive(Debug)]
pub struct Next<'a, T> {
    curr: Option<&'a Node<T>>,
}

impl<T> Clone for Next<'_, T> {
    fn clone(&self) -> Self {
        Self { curr: self.curr }
    }
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
#[derive(Debug)]
pub struct Ancestors<'a, T> {
    curr: Option<&'a Node<T>>,
}

impl<T> Clone for Ancestors<'_, T> {
    fn clone(&self) -> Self {
        Self { curr: self.curr }
    }
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

// SAFETY: mirrors Arc
unsafe impl<T: Send + Sync> Send for Handle<T> {}
unsafe impl<T: Send + Sync> Sync for Handle<T> {}

impl<T> Clone for Handle<T> {
    fn clone(&self) -> Self {
        Self {
            node: self.node,
            arena: self.arena.clone(),
        }
    }
}

impl<T> Handle<T> {
    /// create a new handle
    ///
    /// # Safety
    ///
    /// The given node should be obtained from the given [`Arena`]
    pub(crate) unsafe fn new(node: &Node<T>, arena: &Arc<Arena<T>>) -> Self {
        debug_assert!(
            arena.contains(node),
            "given node does not belong to this arena"
        );
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

impl<T> std::ops::Deref for Handle<T> {
    type Target = Node<T>;

    fn deref(&self) -> &Self::Target {
        // SAFETY: when correctly created(according to Handle::new)
        // this should always be valid
        unsafe { self.node.as_ref() }
    }
}

/// A recursively printing wrapper over a node
pub struct DebugNode<'a, T>(pub &'a Node<T>);

impl<T: Debug> Debug for DebugNode<'_, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let Self(node) = self;

        f.debug_struct("Node")
            .field("index", &node.index)
            .field("value", &node.value)
            .field("children", &DebugChildren(node.children()))
            .finish()
    }
}

struct DebugChildren<'a, T>(Next<'a, T>);

impl<T: Debug> Debug for DebugChildren<'_, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list()
            .entries(self.0.clone().map(DebugNode))
            .finish()
    }
}
