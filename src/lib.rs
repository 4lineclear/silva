#![doc = include_str!("../README.md")]
#![allow(unsafe_code)]
#![deny(
    clippy::all,
    clippy::pedantic,
    clippy::cargo,
    clippy::nursery,
    missing_docs,
    rustdoc::all,
    future_incompatible
)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::ref_as_ptr)]

mod arena;
mod node;

use std::fmt::Display;
use std::num::NonZero;

pub use arena::Arena;
pub use node::*;

// TODO: move back to mainly using indexes again.

// /// example for calling cargo-asm
// #[inline(never)]
// pub fn example() -> Arena<u32> {
//     let a = Arena::new();
//     let root = a.push(None, 0);
//     for i in 0..3 {
//         a.push(root, i);
//     }
//     a
// }

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
        debug_assert!(index <= arena::MAX_INDEX);
        Self(unsafe { NonZero::new_unchecked(index + 1) })
    }

    /// returns the index this arena is stored at
    pub(crate) const fn get(self) -> usize {
        self.0.get() - 1
    }
}

/// A structure you can optionally get a node's index from
///
/// This can be one of:
///
/// - [`Option<Index>`]
/// - [`Index`]
/// - [`Node<T>`]
///
/// # Panics
///
/// Both [`Node`] and [`Index`] index into the given arena, and both will panic
/// if the arena does not contain them. This is to uphold the rule that nodes
/// can only be connected within the same arena.
pub trait AsParent<T>: as_parent::Sealed {
    /// Optionally an index
    fn get(self, arena: &Arena<T>) -> Option<&Node<T>>;
}

impl<T> AsParent<T> for Index {
    fn get(self, arena: &Arena<T>) -> Option<&Node<T>> {
        Some(&arena[self])
    }
}

impl<T> AsParent<T> for Option<Index> {
    fn get(self, arena: &Arena<T>) -> Option<&Node<T>> {
        Some(&arena[self?])
    }
}

impl<T> AsParent<T> for &Node<T> {
    fn get(self, arena: &Arena<T>) -> Option<&Node<T>> {
        if let Some(node) = arena.get(self.index())
            && std::ptr::eq(node, self)
        {
            Some(node)
        } else {
            panic!("node from foreign arena inputted");
        }
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

/// Generate a tree using the given [`Arena`] & values
///
///
/// # Examples
///
/// ```rust
/// # use silva::Arena;
/// // these values will be set by the macro
/// let root;
/// let one;
///
/// let arena = Arena::new();
///
/// silva::tree![
///     &arena,
///     root = ("root") = [
///         ("one") = [],
///         ("two"),
///         ("three"),
///         ("four"),
///         ("five") // added to root
///     ],
///     ("root2") = [
///         one = ("one") = [
///             ("two"),
///             ("three") = [],
///             ("four"),
///             ("five") // added to one
///         ] // added to root2
///     ]
/// ];
///
/// assert_eq!(root.value, "root");
/// assert_eq!(one.value, "one");
///
/// ```
#[macro_export]
macro_rules! tree {
    // thank you serde_json::json!
    ($($tree:tt)*) => {
        $crate::tree_internal![$($tree)*];
    };
}

/// the internal tree implementation
#[macro_export]
#[doc(hidden)]
macro_rules! tree_internal {
    // 0: match empty
    [ $_arena:expr $(, $_name:ident)? $(,)? ] => {};
    // 1: match empty
    [] => {};

    // 2: push child node with default name
    [ $arena:expr, $parent:ident, ($val:expr) $(= [$($inner:tt)*])? ] => {{
        let _node = $arena.push(Some($parent.index()), $val);
        $crate::tree_internal![$arena, _node, $($($inner)*)?];
    }};

    // 3: push child node with given name
    [ $arena:expr, $parent:ident, $name:ident = ($val:expr) $(= [$($inner:tt)*])? ] => {{
        $name = $arena.push(Some($parent.index()), $val);
        $crate::tree_internal![$arena, $name, $($($inner)*)?];
    }};

    // 4: push root node with default name
    [ $arena:expr, ($val:expr) $(= [$($inner:tt)*])? ] => {{
        let _root = $arena.push(None, $val);
        $crate::tree_internal![$arena, _root, $($($inner)*)?];
    }};

    // 5: push root node with given name
    [ $arena:expr, $name:ident = ($val:expr) $(= [$($inner:tt)*])? ] => {{
        $name = $arena.push(None, $val);
        $crate::tree_internal![$arena, $name, $($($inner)*)?];
    }};

    // 6: child nodes
    [   $arena:expr, $parent:ident,
        $(
            $($name:ident = )? ($val:expr) $(= [$($inner:tt)*])?
        ),* $(,)?
    ] => {
        $(
            $crate::tree_internal![
                $arena,
                $parent,
                $($name = )? ($val) $(= [$($inner)*])?
            ];
        )*
    };

    // 7: root nodes
    [   $arena:expr,
        $(
            $($name:ident = )? ($val:expr) $(= [$($inner:tt)*])?
        ),* $(,)?
    ] => {
        $(
            $crate::tree_internal![
                $arena,
                $($name = )? ($val) $(= [$($inner)*])?
            ];
        )*
    };

    // [$($t:tt)*] => {
    //     ::std::compile_error!("unexpected input")
    // };
}
