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
mod index;
mod node;

pub use arena::Arena;
pub use index::{AsParent, Index};
pub use node::*;

pub(crate) use index::AtomicIndex;

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
