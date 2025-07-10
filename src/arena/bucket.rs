use std::ptr::NonNull;
use std::sync::OnceLock;
use std::{alloc, slice};

use super::raw::Location;

pub struct Bucket<T> {
    // NOTE: seems to be about as performant on linux, should check other platforms
    entries: OnceLock<NonNull<T>>,
}

impl<T> Bucket<T> {
    #[expect(clippy::declare_interior_mutable_const)]
    pub const EMPTY: Self = Self {
        entries: OnceLock::new(),
    };

    /// Get an item at `entry`
    ///
    /// # Safety
    ///
    /// The given `entry` must be valid for this bucket.
    pub unsafe fn get(&self, entry: usize) -> Option<&T> {
        // SAFETY: entry soundness upheld by caller
        Some(unsafe { self.entries.get()?.add(entry).as_ref() })
    }

    /// Acquires an item from this entry
    ///
    /// The bucket will be initialized if it is null.
    ///
    /// # Safety
    ///
    /// The given [`Location`] must be valid for this bucket.
    pub unsafe fn acquire(&self, loc: Location) -> &T {
        // SAFETY: loc soundness upheld by caller
        unsafe {
            self.entries
                .get_or_init(|| Self::alloc(Location::capacity(loc.bucket)).0)
                .add(loc.entry)
                .as_ref()
        }
    }

    /// Inititializes a bucket
    ///
    /// # Safety
    ///
    /// The provided length must be non-zero & the correct amount for this bucket.
    /// This bucket's entries must also be uninitialized.
    pub unsafe fn overwrite(&self, len: usize) {
        // SAFETY: len soundness upheld by caller
        let r = self.entries.set(unsafe { Self::alloc(len) }.0);
        debug_assert!(r.is_ok(), "entries overwritten");
    }

    /// Allocate an array of entries of the specified length.
    ///
    /// # Safety
    ///
    /// `len` must be non-zero & the correct amount for the given bucket
    unsafe fn alloc(len: usize) -> (NonNull<T>, alloc::Layout) {
        let layout = alloc::Layout::array::<T>(len).unwrap();
        // SAFETY: len soundness upheld by caller
        NonNull::new(unsafe { alloc::alloc_zeroed(layout) }).map_or_else(
            || alloc::handle_alloc_error(layout),
            |ptr| (ptr.cast(), layout),
        )
    }

    /// Try to dealloc this bucket, does nothing if bucket is `null`.
    ///
    /// # Safety
    ///
    /// This bucket must be correctly allocated
    pub unsafe fn try_dealloc(&mut self, bucket: usize) -> bool {
        let Some(entries) = self.entries.get_mut() else {
            return false;
        };
        let len = Location::capacity(bucket);
        // SAFETY: entry soundness upheld by caller
        drop(unsafe { Box::from_raw(slice::from_raw_parts_mut(entries.as_ptr(), len)) });
        true
    }

    /// Reserve space in this bucket if it is uninit
    ///
    /// # Safety
    ///
    /// `bucket` must refer to this specific bucket
    pub unsafe fn reserve(&self, bucket: usize) {
        // SAFETY: bucket soundness upheld by caller
        self.entries
            .get_or_init(|| unsafe { Self::alloc(Location::capacity(bucket)).0 });
    }

    /// returns `true` if this bucket is allocated
    pub fn is_alloc(&self) -> bool {
        self.entries.get().is_some()
    }
}
