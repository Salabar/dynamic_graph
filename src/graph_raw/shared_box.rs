use core::ptr::{NonNull};
use core::marker::PhantomData;
use core::ops::{Deref, DerefMut};

// Box, but without special treatment from miri. Manages the memory, but does not assume itself to be the only owner of the object.

#[repr(transparent)]
pub (crate) struct SharedBox<T> {
    data: NonNull<T>,
    _ph: PhantomData<T>        
}

impl<T> SharedBox<T> {
    #[inline(always)]
    pub fn new(x: T) -> SharedBox<T> {
        let t = Box::new(x);
        let data = unsafe { NonNull::new_unchecked(Box::into_raw(t)) };
        SharedBox {
            data, _ph : PhantomData
        }
    }

    #[inline(always)]
    pub fn as_ptr(this : &Self) -> *const T {
            this.data.as_ptr() as *const T
    }
}


impl <T> Deref for SharedBox<T> {
    type Target = T;
    #[inline(always)]
    fn deref(&self) -> &T {
        unsafe {
            self.data.as_ref()
        }
    }
}


impl <T> DerefMut for SharedBox<T> {
    #[inline(always)]
    fn deref_mut(&mut self) -> &mut T {
        unsafe {
            self.data.as_mut()
        }
    }
}

impl <T> Drop for SharedBox<T> {
    #[inline(always)]
    fn drop(&mut self) {
        unsafe {
            Box::from_raw(self.data.as_ptr());
        }
    }
}