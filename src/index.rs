use std::fmt::Display;
use std::num::NonZero;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering::{AcqRel, Acquire};

use crate::{Arena, Node};

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
    pub(crate) const unsafe fn new_unchecked(index: usize) -> Self {
        debug_assert!(index <= crate::arena::MAX_INDEX);
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

    pub(crate) const fn none() -> Self {
        Self(AtomicUsize::new(0))
    }

    pub(crate) fn add_child<T>(&self, child: &mut Node<T>) {
        let new = child.index().0.get();
        let mut prev = self.0.load(Acquire);
        loop {
            child.next = NonZero::new(prev).map(Index);

            match self.0.compare_exchange_weak(prev, new, AcqRel, Acquire) {
                Err(next_prev) => prev = next_prev,
                Ok(_) => break,
            }
        }
    }
}

/// A structure you can optionally get a node's index from
///
/// This can be one of:
///
/// - [`Option<Index>`]
/// - [`Index`]
/// - [`Node<T>`]
pub trait AsParent<'a, T>: as_parent::Sealed {
    /// Optionally get an index
    fn get(self, arena: &'a Arena<T>) -> Option<&'a Node<T>>;
}

impl<T> AsParent<'_, T> for Index {
    fn get(self, arena: &Arena<T>) -> Option<&Node<T>> {
        Some(&arena[self])
    }
}

impl<T> AsParent<'_, T> for Option<Index> {
    fn get(self, arena: &Arena<T>) -> Option<&Node<T>> {
        Some(&arena[self?])
    }
}

impl<'a, T> AsParent<'a, T> for &'a Node<T> {
    fn get(self, arena: &'a Arena<T>) -> Option<&'a Node<T>> {
        assert!(arena.contains(self));
        Some(self)
    }
}

mod as_parent {
    pub trait Sealed {}
    impl Sealed for super::Index {}
    impl Sealed for Option<super::Index> {}
    impl<T> Sealed for &super::Node<T> {}
}

impl<T> From<&Node<T>> for Index {
    fn from(value: &Node<T>) -> Self {
        value.index()
    }
}
