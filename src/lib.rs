//! ArrayBuilder makes it easy to dynamically build arrays safely
//! and efficiently.
//!
//! ```
//! use array_builder::ArrayBuilder;
//!
//! struct ArrayIterator<I: Iterator, const N: usize> {
//!     builder: ArrayBuilder<I::Item, N>,
//!     iter: I,
//! }
//!
//! impl<I: Iterator, const N: usize> Iterator for ArrayIterator<I, N> {
//!     type Item = [I::Item; N];
//!
//!     fn next(&mut self) -> Option<Self::Item> {
//!         for _ in self.builder.len()..N {
//!             self.builder.push(self.iter.next()?);
//!         }
//!         self.builder.take().build().ok()
//!     }
//! }
//!
//! impl<I: Iterator, const N: usize> ArrayIterator<I, N> {
//!     pub fn new(i: impl IntoIterator<IntoIter=I>) -> Self {
//!         Self {
//!             builder: ArrayBuilder::new(),
//!             iter: i.into_iter(),
//!         }
//!     }
//!
//!     pub fn remaining(&self) -> &[I::Item] {
//!         &self.builder
//!     }
//! }
//!
//! let mut i = ArrayIterator::new(0..10);
//! assert_eq!(Some([0, 1, 2, 3]), i.next());
//! assert_eq!(Some([4, 5, 6, 7]), i.next());
//! assert_eq!(None, i.next());
//! assert_eq!(&[8, 9], i.remaining());
//! ```

use core::{
    mem::{self, ManuallyDrop, MaybeUninit},
    ops::{Deref, DerefMut},
    ptr, slice,
};
use std::{cmp, fmt};

/// ArrayBuilder makes it easy to dynamically build arrays safely
/// and efficiently.
///
/// ```
/// use array_builder::ArrayBuilder;
///
/// struct ArrayIterator<I: Iterator, const N: usize> {
///     builder: ArrayBuilder<I::Item, N>,
///     iter: I,
/// }
///
/// impl<I: Iterator, const N: usize> Iterator for ArrayIterator<I, N> {
///     type Item = [I::Item; N];
///
///     fn next(&mut self) -> Option<Self::Item> {
///         for _ in self.builder.len()..N {
///             self.builder.push(self.iter.next()?);
///         }
///         self.builder.take().build().ok()
///     }
/// }
///
/// impl<I: Iterator, const N: usize> ArrayIterator<I, N> {
///     pub fn new(i: impl IntoIterator<IntoIter=I>) -> Self {
///         Self {
///             builder: ArrayBuilder::new(),
///             iter: i.into_iter(),
///         }
///     }
///
///     pub fn remaining(&self) -> &[I::Item] {
///         &self.builder
///     }
/// }
///
/// let mut i = ArrayIterator::new(0..10);
/// assert_eq!(Some([0, 1, 2, 3]), i.next());
/// assert_eq!(Some([4, 5, 6, 7]), i.next());
/// assert_eq!(None, i.next());
/// assert_eq!(&[8, 9], i.remaining());
/// ```
pub struct ArrayBuilder<T, const N: usize> {
    buf: [MaybeUninit<T>; N],
    len: usize,
}

impl<T: fmt::Debug, const N: usize> fmt::Debug for ArrayBuilder<T, N> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ArrayBuilder")
            .field("capacity", &N)
            .field("length", &self.len)
            .field("values", &self.deref())
            .finish()
    }
}

impl<T, U, const N: usize> cmp::PartialEq<ArrayBuilder<U, N>> for ArrayBuilder<T, N>
where
    T: cmp::PartialEq<U>,
{
    fn eq(&self, other: &ArrayBuilder<U, N>) -> bool {
        self.deref() == other.deref()
    }
}
impl<T: cmp::Eq, const N: usize> cmp::Eq for ArrayBuilder<T, N> {}
impl<T: cmp::PartialOrd, const N: usize> cmp::PartialOrd for ArrayBuilder<T, N> {
    fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
        self.deref().partial_cmp(other.deref())
    }
}
impl<T: cmp::Ord, const N: usize> cmp::Ord for ArrayBuilder<T, N> {
    fn cmp(&self, other: &Self) -> cmp::Ordering {
        self.deref().cmp(other.deref())
    }
}

impl<T, const N: usize> Drop for ArrayBuilder<T, N> {
    fn drop(&mut self) {
        self.clear()
    }
}

impl<T, const N: usize> Deref for ArrayBuilder<T, N> {
    type Target = [T];
    fn deref(&self) -> &[T] {
        unsafe { slice::from_raw_parts(self.as_ptr(), self.len) }
    }
}

impl<T, const N: usize> DerefMut for ArrayBuilder<T, N> {
    fn deref_mut(&mut self) -> &mut [T] {
        unsafe { slice::from_raw_parts_mut(self.as_mut_ptr(), self.len) }
    }
}

impl<T, const N: usize> From<[T; N]> for ArrayBuilder<T, N> {
    fn from(array: [T; N]) -> Self {
        Self {
            buf: unsafe { ptr::read(array.as_ptr() as *const [MaybeUninit<T>; N]) },
            len: N,
        }
    }
}

impl<T, const N: usize> ArrayBuilder<T, N> {
    const UNINIT: MaybeUninit<T> = MaybeUninit::uninit();

    /// Create a new ArrayBuilder, backed by an uninitialised [T; N]
    pub fn new() -> Self {
        Self {
            buf: [Self::UNINIT; N],
            len: 0,
        }
    }

    /// Get the number of initialized values in the ArrayBuilder
    pub fn len(&self) -> usize {
        self.len
    }

    /// Get whether the ArrayBuilder is full
    pub fn is_full(&self) -> bool {
        self.len == N
    }

    /// Get whether the ArrayBuilder is empty
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Empties the ArrayBuilder
    pub fn clear(&mut self) {
        let s: &mut [T] = self;
        unsafe {
            ptr::drop_in_place(s);
        }
        self.len = 0;
    }

    fn as_ptr(&self) -> *const T {
        self.buf.as_ptr() as _
    }

    fn as_mut_ptr(&mut self) -> *mut T {
        self.buf.as_mut_ptr() as _
    }

    /// Pushes the value onto the ArrayBuilder
    ///
    /// Panics:
    /// If the ArrayBuilder is full
    pub fn push(&mut self, t: T) {
        assert!(self.len < N);
        unsafe {
            self.push_unchecked(t);
        }
    }

    /// Pushes the value onto the ArrayBuilder if there is space
    /// Otherwise, returns Err(t)
    pub fn try_push(&mut self, t: T) -> Result<(), T> {
        if self.len < N {
            unsafe {
                self.push_unchecked(t);
            }
            Ok(())
        } else {
            Err(t)
        }
    }

    /// Pushes the value onto the ArrayBuilder
    ///
    /// Safety:
    /// The ArrayBuilder must not be full
    pub unsafe fn push_unchecked(&mut self, t: T) {
        ptr::write(self.as_mut_ptr().add(self.len), t);
        self.len += 1;
    }

    /// Pops the last value on the ArrayBuilder
    /// Returns None if the ArrayBuilder is empty
    ///
    /// ```
    /// use array_builder::ArrayBuilder;
    /// let mut builder: ArrayBuilder<usize, 4> = [1, 2, 3, 4].into();
    /// let t = builder.pop().unwrap();
    /// builder.push(t * t);
    /// assert_eq!(Ok([1, 2, 3, 16]), builder.build());
    /// ```
    pub fn pop(&mut self) -> Option<T> {
        if self.len > 0 {
            unsafe { Some(self.pop_unchecked()) }
        } else {
            None
        }
    }

    /// Pops the last value on the ArrayBuilder
    ///
    /// Safety:
    /// The ArrayBuilder must not be empty
    pub unsafe fn pop_unchecked(&mut self) -> T {
        self.len -= 1;
        ptr::read(self.as_ptr().add(self.len))
    }

    /// Converts the ArrayBuilder into a [T; N].
    /// If the ArrayBuilder is not full, returns Err(self)
    pub fn build(self) -> Result<[T; N], Self> {
        if self.len == N {
            unsafe { Ok(self.build_unchecked()) }
        } else {
            Err(self)
        }
    }

    /// Converts the ArrayBuilder into a [T; N].
    ///
    /// Safety:
    /// The ArrayBuilder must be full
    pub unsafe fn build_unchecked(self) -> [T; N] {
        let self_ = ManuallyDrop::new(self);
        ptr::read(self_.as_ptr() as *const [T; N])
    }

    /// Takes the value out of the ArrayBuilder
    /// Leaving an empty ArrayBuilder in it's place.
    ///
    /// ```
    /// use array_builder::ArrayBuilder;
    /// let mut builder1: ArrayBuilder<usize, 4> = [1, 2, 3, 4].into();
    /// assert!(builder1.is_full());
    /// let builder2 = builder1.take();
    /// assert!(builder1.is_empty());
    /// assert!(builder2.is_full());
    /// ```
    pub fn take(&mut self) -> Self {
        mem::replace(self, Self::new())
    }
}
