use super::*;

use std::collections::VecDeque;
#[derive(PartialEq, Eq)]
pub struct GraphRaw<T> {
    pub(crate) data : Vec<Box<T>>,
    pub(crate) cleanup_gen : CleanupGen,
}

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

    // Moves internal pointer to the start of the storage
    pub(crate) unsafe fn touch(&mut self, index : usize, item : *mut NodeType) -> bool
    {
        let item_index = {
            let s = (*item).service_mut();
            if (s.cleanup_gen == self.cleanup_gen) {
                return false;
            }
            s.cleanup_gen = self.cleanup_gen;
            s.store_index
        };

        if (item_index != index) {
            self.data[index].service_mut().store_index = item_index;
            {
                //previous reference already dropped
                (*item).service_mut().store_index = index;
            }

            self.data.swap(item_index, index);
        }
        true
    }

    //GraphPtr here and later never dangles because there is no safe way to create
    //one after anchor branded with the same 'id is dropped and there is no safe way to dispose of the nodes
    //before it happens
    //Every reference bound to &self is protected from aliasing due to Rust borrowing rules
    pub(crate) fn get<'id>(&self, item : GraphPtr<'id, NodeType>) -> &N
    {
        unsafe {
            (*item.as_ptr()).get()
        }
    }

    pub(crate) fn get_mut<'id>(&mut self, item : GraphPtr<'id, NodeType>) -> &mut N
    {
        unsafe {
            (*item.as_mut()).get_mut()
        }
    }

    pub(crate) unsafe fn kill(&mut self, item : *const NodeType)
    {
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
        let g = src._guard;
        let current = src.node.as_ptr() as *const NodeType;
        iter.map(move |x| {
            let ptr = x.0;
            let p =  unsafe { GraphPtr::from_ptr(ptr, g) };
            let that = unsafe { (*ptr).get() };
        
            if current == ptr {
                GraphIterRes { values : Loop(EdgeSingle { this : that, edge : x.1}), ptr : p }
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
        let current = src.node.as_ptr() as *mut NodeType;
        iter.map(move |x| {
            let ptr = x.0;
            let p =  unsafe { GraphPtr::from_mut(ptr, g) };
            let that = unsafe { (*ptr).get_mut() };

            if current == ptr {
                GraphIterRes { values : Loop(EdgeSingle { this : that, edge : x.1}), ptr : p }
            } else {
                //aliasing was explicitly checked
                let this = unsafe { (*current).get_mut() };
                GraphIterRes { values : Both(EdgeBoth { this, that, edge : x.1 }), ptr : p }
            }
        })
    }
}

impl <N, E> GraphRaw<NamedNode<N, E>>
{
    pub(crate) fn cleanup_precise<'a, Iter : 'a>(&'a mut self, root : Iter)
        where Iter : Iterator<Item = *mut NamedNode<N, E>>
    {
        self.cleanup_gen.flip();
        let mut index = 0;
        let mut queue = VecDeque::new();
        for i in root {
            unsafe {
                if self.touch(index, i) {
                    index += 1;
                    queue.push_back(i);
                }
            }
        }

        while (!queue.is_empty()) {
            unsafe {
                let q = queue.pop_front().unwrap();
                let iter = {
                    //*q is dropped after this line and will not alias
                    (*q).refs.iter_mut().map(|x| { x.0.as_mut() })
                };

                for i in iter {
                    if self.touch(index, q) {
                        index += 1;
                        queue.push_back(q);
                    }
                }
            }
        }
    }

    pub(crate) fn bridge<'id>(&mut self, src : GraphPtr<'id, node_views::NamedNode<'id, N, E>>,
                              dst : GraphPtr<'id, node_views::NamedNode<'id, N, E>>)
                    -> Option<(&'_ mut node_views::NamedNode<'id, N, E>, &'_ mut node_views::NamedNode<'id, N, E>)>
    {
        if src == dst { 
            None
        } else {
            unsafe {
                //node_view::_ is a prefix of _ and both are repr(C)
                let src = transmute(&mut (*src.as_mut()));
                let dst = transmute(&mut (*dst.as_mut()));
                Some((src, dst))
            }
        }
    }

    pub(crate) fn get_edge<'id>(&self, src : GraphPtr<'id, NamedNode<N, E>>, dst : GraphPtr<'id, NamedNode<N, E>>) -> Option<Edge<&'_ N, &'_ E>>
    {
        let this = unsafe { &(*src.as_ptr()) };

        let this_refs = &this.refs;
        let this = &this.data;

        let s_dst = unsafe { dst.into_static() };
        if let Some(e) = this_refs.get(&s_dst) {
            if src == dst {
                Some(Loop(EdgeSingle { this, edge : &e }))
            } else {
                let that = self.get(dst);
                Some(Both(EdgeBoth { this, that, edge : &e }))
            }
        } else {
            None
        }
    }

    pub(crate) fn get_edge_mut<'id>(&mut self, src : GraphPtr<'id, NamedNode<N, E>>, dst : GraphPtr<'id, NamedNode<N, E>>) -> Option<Edge<&'_ mut N, &'_ mut E>>
    {
        //aliasing check will be required in order to not violate (*) invariants
        let this = unsafe { &mut (*src.as_mut()) };

        let this_refs = &mut this.refs;
        let this = &mut this.data;
        
        let s_dst = unsafe { dst.into_static() };
        if let Some(e) = this_refs.get_mut(&s_dst) {
            if src == dst {
                Some(Loop(EdgeSingle { this, edge : e }))
            } else {
                let that = self.get_mut(dst); // (*)
                Some(Both(EdgeBoth { this, that, edge : e }))
            }
        } else {
            None
        }
    }

    pub(crate) fn get_view<'id>(&self, dst : GraphPtr<'id, NamedNode<N, E>>) -> &node_views::NamedNode<'id, N, E>
    {
        unsafe {
            transmute(&*dst.as_ptr())
        }
    }

    pub(crate) fn get_view_mut<'id>(&mut self, dst : GraphPtr<'id, NamedNode<N, E>>) -> &mut node_views::NamedNode<'id, N, E>
    {
        unsafe {
            transmute(&mut *dst.as_mut())
        }
    }

    pub(crate) fn iter<'a, 'id : 'a>(&'a self, dst : GraphPtr<'id, NamedNode<N, E>>) -> impl Iterator<Item = GraphIterRes<Edge<&'a N, &'a E>, GraphPtr<'id, NamedNode<N, E>>>>
    {
        let current = dst.as_ptr();
        let node_refs = unsafe { &(*current).refs };
        self.iter_from_raw(dst, node_refs.iter().map(|x| {
            let p = x.0.as_ptr();
            (p, x.1)
        }))
    }

    pub(crate) fn iter_mut<'a, 'id : 'a>(&'a mut self, dst : GraphPtr<'id, NamedNode<N, E>>) -> impl Iterator<Item = GraphIterRes<Edge<&'a mut N, &'a mut E>, GraphPtr<'id, NamedNode<N, E>>>>
    {
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
