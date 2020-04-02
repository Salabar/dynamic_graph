use super::*;

use std::collections::VecDeque;
#[derive(PartialEq, Eq)]
pub struct GraphRaw<T> {
    pub(crate) data : Vec<Box<T>>,
    pub(crate) cleanup_gen : CleanupGen,
}

//Invariant Q: A graph node only contains references to existing nodes.

//Theorem W: A GraphPtr never dangles.
// 1. There is no safe way to create one after parent anchor is dropped.
// 2. There is no way to dereference a GraphPtr after anchor is dropped.

//Corollary E: Dereferencing a pointer to a graph node in a non-recursive function is safe as long
//as aliasing in the function body is prevented.
// 1. (W)
// 2. A reference bound to &self/&mut self  is dropped when another function bound to &mut self is called.
// 3. A GraphPtr can only be dereferenced by calling a function bound to self.

//Theorem R: Transmuting a reference to NamedNode into a reference to node_views::NameNode (etc) and back is safe.
// 1. Both structures are repr(C)
// 2. node_views::_ is a strict prefix of the _.
// 3. Alignment of _ of greater or equals to that of node_views::_.

impl <'a, N : 'a, E : 'a, NodeType> GraphRaw<NodeType>
where NodeType : GraphNode<Node = N, Edge = E>
{
    pub(crate) fn spawn_detached(&mut self, data : N) -> *const NodeType
    {
        let mut node = Box::new(NodeType::from_data(data));
        node.service_mut().store_index = self.data.len();
        node.service_mut().cleanup_gen = self.cleanup_gen;

        let ptr : *const NodeType = &*node;

        self.data.push(node);
        ptr
    }

    // Moves internal pointer to the start of the storage, requires item to be a valid pointer
    pub(crate) unsafe fn touch(&mut self, index : usize, item : *mut NodeType) -> bool
    {
        let item_index = {
            let s = (*item).service_mut();
            if s.cleanup_gen == self.cleanup_gen {
                return false;
            }
            s.cleanup_gen = self.cleanup_gen;
            s.store_index
        };

        {
            //previous reference already dropped
            (*item).service_mut().store_index = index;
        }

        self.data[index].service_mut().store_index = item_index;
        self.data.swap(item_index, index);

        true
    }

    pub(crate) fn get<'id>(&self, item : GraphPtr<'id, NodeType>) -> &N
    {
        // (Q, W, E)
        unsafe {
            (*item.as_ptr()).get()
        }
    }

    pub(crate) fn get_mut<'id>(&mut self, item : GraphPtr<'id, NodeType>) -> &mut N
    {
        // (Q, W, E)
        unsafe {
            (*item.as_mut()).get_mut()
        }
    }

    pub(crate) unsafe fn kill(&mut self, item : *const NodeType)
    {
        // (Q, W, E)
        let store_index = {
            (*item).service().store_index
        };
        
        if self.data.len() > 0 && store_index < self.data.len() {
            self.data.last_mut().unwrap().service_mut().store_index = store_index;
            self.data.swap_remove(store_index);
        } else { 
            unreachable!()
            //storeIndex always points to the current position in the Vec
            //unreachable_unchecked()
        }
    }

    pub(crate) fn iter_from_raw<'id : 'a, Iter : 'a>(&'a self, src : GraphPtr<'id, NodeType>, iter : Iter)
        -> impl Iterator<Item = GraphIterRes<Edge<&'a N, &'a E>, GraphPtr<'id, NodeType>>>
    where Iter : Iterator<Item = (*const NodeType, &'a E)>
    {
        // (Q, W, E)
        let g = src._guard;
        let current = src.as_ptr();
        iter.map(move |x| {
            let ptr = x.0;
            let p =  unsafe { GraphPtr::from_ptr(ptr, g) };
            let that = unsafe { (*ptr).get() };
        
            if current == ptr {
                GraphIterRes { values : Loop(EdgeLoop { this : that, edge : x.1}), ptr : p }
            } else {
                let this = unsafe { (*current).get() };
                GraphIterRes { values : Both(EdgeBoth { this, that, edge : x.1 }), ptr : p }
            }
        })
    }

    pub(crate) fn iter_mut_from_raw<'id : 'a, Iter : 'a>(&'a mut self, src : GraphPtr<'id, NodeType>, iter : Iter)
        -> impl Iterator<Item = GraphIterRes<Edge<&'a mut N, &'a mut E>, GraphPtr<'id, NodeType>>>
    where Iter : Iterator<Item = (*mut NodeType, &'a mut E)>
    {
        let g = src._guard;
        let current = src.as_mut();
        // (Q, W, E)
        iter.map(move |x| {
            let ptr = x.0;
            let p =  unsafe { GraphPtr::from_mut(ptr, g) };
            let that = unsafe { (*ptr).get_mut() };

            if current == ptr {
                GraphIterRes { values : Loop(EdgeLoop { this : that, edge : x.1}), ptr : p }
            } else {
                //aliasing was explicitly checked
                let this = unsafe { (*current).get_mut() };
                GraphIterRes { values : Both(EdgeBoth { this, that, edge : x.1 }), ptr : p }
            }
        })
    }

    pub(crate) fn cleanup_precise<'b>(&'b mut self, root : Box<dyn Iterator<Item = *mut NodeType> + 'b>)
    {
        //Move every accessible node to the beginning of the internal Vec and drop the rest
        self.cleanup_gen.flip();
        let mut index = 0;
        let mut queue = VecDeque::new();
        for i in root {
            unsafe {
                //Q
                if self.touch(index, i) {
                    index += 1;
                    queue.push_back(i);
                }
            }
        }

        while !queue.is_empty() {
            unsafe {
                let q = queue.pop_front().unwrap();
                let iter = {
                    //*q is dropped after this line and will not alias
                    (*q).edge_ptrs()
                };

                for i in iter {
                    // (Q, W, E)
                    if self.touch(index, i) {
                        index += 1;
                        queue.push_back(i);
                    }
                }
            }
        }
        self.data.truncate(index);
        self.data.shrink_to_fit();
    }
}

impl <N, E> GraphRaw<NamedNode<N, E>>
{
    pub(crate) fn bridge<'id>(&mut self, src : GraphPtr<'id, NamedNode<N, E>>,
                                         dst : GraphPtr<'id, NamedNode<N, E>>)
        -> Option<(&'_ mut node_views::NamedNode<'id, N, E>, &'_ mut node_views::NamedNode<'id, N, E>)>
    {
        assert_eq!(std::mem::align_of::<node_views::NamedNode<'id, N, E>>(),
                   std::mem::align_of::<NamedNode<N, E>>()
        );

        if src != dst { 
            unsafe {
                //(R)
                let src = transmute(&mut (*src.as_mut()));
                let dst = transmute(&mut (*dst.as_mut()));
                Some((src, dst))
            }
        } else {
            None
        }
    }

    pub(crate) fn get_edge<'id>(&self, src : GraphPtr<'id, NamedNode<N, E>>, dst : GraphPtr<'id, NamedNode<N, E>>) -> Option<Edge<&'_ N, &'_ E>>
    {
        //(Q, W, E)
        let (this_refs, this) = unsafe {
            let n = &(*src.as_ptr());
            (&n.refs, &n.data)
        };

        let s_dst = dst.into_static();

        if let Some(e) = this_refs.get(&s_dst) {
            if src == dst {
                Some(Loop(EdgeLoop { this, edge : &e }))
            } else {
                let that = self.get(dst);
                Some(Both(EdgeBoth { this, that, edge : &e }))
            }
        } else {
            None
        }
    }

    pub(crate) fn get_edge_mut<'id>(&mut self, src : GraphPtr<'id, NamedNode<N, E>>, dst : GraphPtr<'id, NamedNode<N, E>>)
                  -> Option<Edge<&'_ mut N, &'_ mut E>>
    {
        //(Q, W, E)
        //aliasing check will be required in order to not violate (*) invariants
        let (this_refs, this) = unsafe {
            let n = &mut (*src.as_mut());
            (&mut n.refs, &mut n.data)
        };

        let s_dst = dst.into_static();

        if let Some(e) = this_refs.get_mut(&s_dst) {
            if src == dst {
                Some(Loop(EdgeLoop { this, edge : e }))
            } else {
                let that = self.get_mut(dst); // (*)
                Some(Both(EdgeBoth { this, that, edge : e }))
            }
        } else {
            None
        }
    }

    pub(crate) fn get_view<'id>(&self, dst : GraphPtr<'id, NamedNode<N, E>>) -> &node_views::NamedNode<'id, N, E>
    {   //(R)
        unsafe {
            transmute(&*dst.as_ptr())
        }
    }

    pub(crate) fn get_view_mut<'id>(&mut self, dst : GraphPtr<'id, NamedNode<N, E>>) -> &mut node_views::NamedNode<'id, N, E>
    {
        //(R)
        unsafe {
            transmute(&mut *dst.as_mut())
        }
    }

    pub(crate) fn iter<'a, 'id : 'a>(&'a self, dst : GraphPtr<'id, NamedNode<N, E>>)
        -> impl Iterator<Item = GraphIterRes<Edge<&'a N, &'a E>, GraphPtr<'id, NamedNode<N, E>>>>
    {
        //(Q, W, E)
        let current = dst.as_ptr();
        let node_refs = unsafe { &(*current).refs };
        self.iter_from_raw(dst, node_refs.iter().map(|x| {
            let p = x.0.as_ptr();
            (p, x.1)
        }))
    }

    pub(crate) fn iter_mut<'a, 'id : 'a>(&'a mut self, dst : GraphPtr<'id, NamedNode<N, E>>)
        -> impl Iterator<Item = GraphIterRes<Edge<&'a mut N, &'a mut E>, GraphPtr<'id, NamedNode<N, E>>>>
    {
        //(Q, W, E)
        let current = dst.as_mut();
        //*current is dropped before closure is ever invoked and does not alias
        let node_refs = unsafe { &mut (*current).refs };
        self.iter_mut_from_raw(dst, node_refs.iter_mut().map(|x| {
            let p = x.0.as_mut();
            (p, x.1)
        }))
    }
}

impl <T> GraphRaw<T> {
    pub(crate) fn new() -> GraphRaw<T>
    {
        GraphRaw { data : Vec::new(), cleanup_gen : CleanupGen::Even }
    }
}
