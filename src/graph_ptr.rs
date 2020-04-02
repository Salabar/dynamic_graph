use super::*;
pub use generativity::*;

/// A checked pointer type used to access and traverse graph nodes in the crate. This pointer cannot be dereferenced
/// and requires the parent anchor object to access the data stored in the collection.
#[repr(transparent)]
pub struct GraphPtr<'id, T> {
    pub(crate) node : NonNull<T>,
    pub(crate) _guard : Id<'id>
}

impl <'id, T> PartialEq for GraphPtr<'id, T> {
    fn eq(&self, other : &Self) -> bool
    {
        self.node == other.node
    }
}

impl <'id, T> Eq for GraphPtr<'id, T> {}

impl <'id, T> GraphPtr<'id, T> {
    pub(crate) fn as_mut(self) -> *mut T
    {
        self.node.as_ptr()
    }

    /// Returns a raw pointer to the graph node. This pointer should not be dereferenced directly and is meant
    /// to be a way to cache GraphPtrs between cleanups.
    pub fn as_ptr(self) -> *const T
    {
        self.node.as_ptr() as *const T
    }

    //ptr must be a valid pointer.
    //node behind ptr must belong to the same graph as an 'id branded anchor.
    pub(crate) unsafe fn from_mut(ptr : *mut T, guard : Id<'id>) -> Self
    {
        GraphPtr { node : NonNull::new_unchecked(ptr), _guard : guard }
    }

    pub(crate) unsafe fn from_ptr(ptr : *const T, guard : Id<'id>) -> Self
    {
        GraphPtr { node : NonNull::new_unchecked(ptr as *mut T), _guard : guard }
    }

    pub(crate) fn into_static(self) -> GraphPtr<'static, T>
    {
        unsafe{
            transmute(self)
        }
    }
}

impl <'id, T> Hash for GraphPtr<'id, T>  {
    fn hash<H: Hasher>(&self, state: &mut H)
    {
        self.node.hash(state);
    }
}

impl <'id, T> Clone for GraphPtr<'id, T> {
    fn clone(&self) -> GraphPtr<'id, T>
    {
        GraphPtr { node : self.node, _guard : self._guard }
    }
}

impl <'id, T> Copy for GraphPtr<'id, T> {}
