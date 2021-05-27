use core::{
    ptr,
    slice,
    mem::{self, ManuallyDrop, MaybeUninit},
    ops::{Deref, DerefMut},
};
pub struct ArrayBuilder<T, const N: usize> {
    buf: [MaybeUninit<T>; N],
    len: usize,
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

impl<T, const N: usize> ArrayBuilder<T, N> {
    const UNINIT: MaybeUninit<T> = MaybeUninit::uninit();

    pub fn new() -> Self {
        Self {
            buf: [Self::UNINIT; N],
            len: 0,
        }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_full(&self) -> bool {
        self.len == N
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

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

    pub fn push(&mut self, t: T) {
        assert!(self.len < N);
        unsafe { self.push_unchecked(t); }
    }

    pub fn try_push(&mut self, t: T) -> Result<(), T> {
        if self.len < N {
            unsafe { self.push_unchecked(t); }
            Ok(())
        } else {
            Err(t)
        }
    }

    pub unsafe fn push_unchecked(&mut self, t: T) {
        ptr::write(self.as_mut_ptr().add(self.len), t);
        self.len += 1;
    }

    pub fn pop(&mut self) -> Option<T> {
        if self.len > 0 {
            unsafe { Some(self.pop_unchecked()) }
        } else {
            None
        }
    }

    pub unsafe fn pop_unchecked(&mut self) -> T {
        self.len -= 1;
        ptr::read(self.as_ptr().add(self.len))
    }

    pub fn build(self) -> Result<[T; N], Self> {
        if self.len == N {
            unsafe { Ok(self.build_unchecked()) }
        } else {
            Err(self)
        }
    }

    pub unsafe fn build_unchecked(self) -> [T; N] {
        let self_ = ManuallyDrop::new(self);
        ptr::read(self_.as_ptr() as *const [T; N])
    }

    pub fn take(&mut self) -> Self {
        mem::replace(self, Self::new())
    }
}
