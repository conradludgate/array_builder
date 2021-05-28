//! ArrayBuilder makes it easy to dynamically build arrays safely
//! and efficiently.
//!
//! Dynamic array initialisation is very dangerous currently.
//! The safest way is to initialize one with a default value
//!
//! ```
//! let mut array = [0; 32];
//! for i in 0..32 {
//!     array[i] = i;
//! }
//! ```
//!
//! This is not possible in general though. For any type `[T; N]`,
//! T either needs to be [`Copy`], or there needs to be a `const t: T`.
//! This is definitely not always the case.
//!
//! The second problem is efficiency. In the example above, we are
//! filling an array with zeros, just to replace them. While the
//! compiler can sometimes optimise this away, it's nice to have the guarantee.
//!
//! So, what's the alternative? How about [`MaybeUninit`]! Although, it's not that simple.
//! Take the following example, which uses completely safe Rust! Can you spot the error?
//!
//! ```should_panic
//! # #![feature(maybe_uninit_uninit_array)]
//! # #![feature(maybe_uninit_extra)]
//! # use std::mem::MaybeUninit;
//! let mut uninit: [MaybeUninit<String>; 8] = MaybeUninit::uninit_array();
//! uninit[0].write("foo".to_string());
//! uninit[1].write("bar".to_string());
//! uninit[2].write("baz".to_string());
//! panic!("oops");
//! ```
//!
//! Did you spot it? Right there is a memory leak. The key here is that
//! [`MaybeUninit`] **does not** implement [`Drop`]. This makes sense
//! since the value could be uninitialized, and calling [`Drop`] on an
//! uninitialized value is undefined behaviour. The result of this is that
//! the 3 [`String`] values we did initialize never got dropped!
//! Now, this is safe according to Rust. Leaking memory is not undefined
//! behaviour. But it's still not something we should promote.
//!
//! What other options do we have? The only solution is to provide a new
//! `struct` that wraps the array, and properly implements [`Drop`]. That
//! way, if `drop` is called, we can make sure any initialized values get
//! dropped properly. This is exactly what [`ArrayBuilder`] provides.
//!
//! ```should_panic
//! use array_builder::ArrayBuilder;
//! let mut uninit: ArrayBuilder<String, 8> = ArrayBuilder::new();
//! uninit.push("foo".to_string());
//! uninit.push("bar".to_string());
//! uninit.push("baz".to_string());
//! panic!("oops"); // ArrayBuilder drops the 3 values above for you
//! ```
//!
//! ```
//! use array_builder::ArrayBuilder;
//! let mut uninit: ArrayBuilder<String, 3> = ArrayBuilder::new();
//! uninit.push("foo".to_string());
//! uninit.push("bar".to_string());
//! uninit.push("baz".to_string());
//! let array: [String; 3] = uninit.build().unwrap();
//! ```

use core::{
    cmp, fmt,
    mem::{self, ManuallyDrop, MaybeUninit},
    ops::{Deref, DerefMut},
    ptr, slice,
};

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

impl<T: Clone, const N: usize> Clone for ArrayBuilder<T, N> {
    fn clone(&self) -> Self {
        let mut new = Self::new();
        new.len = self.len();
        new.deref_mut().clone_from_slice(self.deref());
        new
    }
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
