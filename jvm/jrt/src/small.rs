//! A fixed-length slice that keeps up to `N` elements inside the object
//! itself, so a small Java object takes one allocation.
use std::mem::MaybeUninit;
use std::ops::Deref;

pub struct Small<T, const N: usize> {
    len: u32,
    /// Set when there are more than `N` elements.
    heap: Option<Box<[T]>>,
    inline: [MaybeUninit<T>; N],
}

impl<T, const N: usize> Small<T, N> {
    pub fn empty() -> Self {
        Self::new_with(0, || unreachable!())
    }

    /// `len` elements, each made by `f`.
    pub fn new_with(len: usize, mut f: impl FnMut() -> T) -> Self {
        let mut s = Small {
            len: 0,
            heap: None,
            inline: [const { MaybeUninit::uninit() }; N],
        };
        if len > N {
            s.heap = Some((0..len).map(|_| f()).collect());
            s.len = len as u32;
        } else {
            for slot in s.inline.iter_mut().take(len) {
                slot.write(f());
                s.len += 1;
            }
        }
        s
    }
}

impl<T, const N: usize> Deref for Small<T, N> {
    type Target = [T];

    #[inline]
    fn deref(&self) -> &[T] {
        match &self.heap {
            Some(b) => b,
            // The first `len` are initialized.
            None => unsafe {
                std::slice::from_raw_parts(self.inline.as_ptr().cast::<T>(), self.len as usize)
            },
        }
    }
}

impl<T, const N: usize> Drop for Small<T, N> {
    fn drop(&mut self) {
        if self.heap.is_none() {
            for slot in &mut self.inline[..self.len as usize] {
                unsafe { slot.assume_init_drop() };
            }
        }
    }
}

impl<T: Clone, const N: usize> Clone for Small<T, N> {
    fn clone(&self) -> Self {
        let mut it = self.iter();
        Small::new_with(self.len(), || it.next().expect("same length").clone())
    }
}
