use super::*;

pub mod shared_box;
use crate::shared_box::*;

use std::collections::VecDeque;

pub (crate) struct GraphRaw<T> {
    pub(crate) data : Vec<SharedBox<T>>,
    pub(crate) cleanup_gen : CleanupGen,
}

pub struct CleanupState<'this, T> 
{
    parent : &'this mut GraphRaw<T>,
    queue : VecDeque<*mut T>,
    index : usize
}

impl <NodeType> CleanupState<'_, NodeType>
where NodeType : GraphNode
{
    pub fn touch(&mut self, node : *mut NodeType) {
        if self.parent.touch(self.index, node) {
            self.index += 1;
            self.queue.push_back(node);
        }
    }
}

//Invariant Q: A graph node only contains references to existing nodes.

//Theorem W: A GraphPtr never dangles.
// 1. There is no way to create one after parent anchor is dropped.
// 2. There is public API to access GraphRaw data directly.
// 3. Nodes are only dropped when anchor is.

//Theorem E: Dereferencing a pointer to a graph node in a non-recursive function is safe as long
//as mutable aliasing in the function body is prevented.
// 1. (W)
// 2. A reference bound to &self/&mut self  is dropped when another function bound to &mut self is called.
// 3. A GraphPtr can only be dereferenced by calling a function bound to &self.

impl <'a, N : 'a, NodeType> GraphRaw<NodeType>
where NodeType : GraphNode<Node = N>
{
    pub(crate) fn spawn_detached(&mut self, data : N) -> *const NodeType
    {
        let mut node = SharedBox::new(NodeType::from_data(data));
        node.meta_mut().store_index = self.data.len();
        node.meta_mut().cleanup_gen = self.cleanup_gen;

        let ptr = SharedBox::as_ptr(&node);

        self.data.push(node);
        ptr
    }

    // Moves internal pointer to the start of the storage, requires item to be a valid pointer
    // This function is used in the preparatory stage of cleanup before any node is dropped therefore (Q W E)
    // apply.
    pub(crate) fn touch(&mut self, index : usize, item : *mut NodeType) -> bool
    {
        let item_index = {
            let s = unsafe { (*item).meta_mut() };
            if s.cleanup_gen == self.cleanup_gen {
                return false;
            }
            s.cleanup_gen = self.cleanup_gen;
            s.store_index
        };

        unsafe {
            //previous dereferencing already dropped
            (*item).meta_mut().store_index = index;
        }

        self.data[index].meta_mut().store_index = item_index;
        self.data.swap(item_index, index);

        true
    }

    pub(crate) fn get<'id>(&self, item : GraphPtr<'id, NodeType>) -> &N
    {
        // (E)
        unsafe {
            (*item.as_ptr()).get()
        }
    }

    pub(crate) fn get_mut<'id>(&mut self, item : GraphPtr<'id, NodeType>) -> &mut N
    {
        // (E)
        unsafe {
            (*item.as_mut()).get_mut()
        }
    }

    pub(crate) unsafe fn kill(&mut self, item : *const NodeType)
    {
        // (E)
        let store_index = {
            (*item).meta().store_index
        };
        
        if self.data.len() > 0 && store_index < self.data.len() {
            self.data.last_mut().unwrap().meta_mut().store_index = store_index;
            self.data.swap_remove(store_index);
        } else { 
            unreachable!()
            //storeIndex always points to the current position in the Vec
            //unreachable_unchecked()
        }
    }

    pub(crate) fn iter_from_raw<'id : 'a, Iter : 'a, E : 'a>(&'a self, src : GraphPtr<'id, NodeType>, iter : Iter)
        -> impl Iterator<Item = GraphIterRes<Edge<&'a N, &'a E>, GraphPtr<'id, NodeType>>>
    where Iter : Iterator<Item = (*const NodeType, &'a E)>
    {
        let g = src._guard;
        let current = src.as_ptr();
        iter.map(move |x| {
            let ptr = x.0;
            //(W)
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

    pub(crate) fn iter_mut_from_raw<'id : 'a, Iter : 'a, E: 'a>(&'a mut self, src : GraphPtr<'id, NodeType>, iter : Iter)
        -> impl Iterator<Item = GraphIterRes<Edge<&'a mut N, &'a mut E>, GraphPtr<'id, NodeType>>>
    where Iter : Iterator<Item = (*mut NodeType, &'a mut E)>
    {
        let g = src._guard;
        let current = src.as_mut();
        // (E)
        iter.map(move |x| {
            let ptr = x.0;
            //(W)
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

    pub(crate) fn cleanup_precise<'id>(&mut self, root : &impl RootCollection<'id, NodeType>)
    {
        self.cleanup_gen.flip();
        let mut state = CleanupState { parent : self, index : 0, queue : VecDeque::new() };
        RootCollection::traverse(root, &mut state);

        while let Some(q) = state.queue.pop_front() {
            unsafe {
                (*q).traverse(&mut state);
            }
        }
        //Every accessible node is stored before index.
        let index = state.index;
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
        if src != dst { 
            //this transmute only affects lifetime parameter
            let src = unsafe { (*src.as_mut()).get_view_mut() };
            let dst = self.get_view_mut(dst);
            Some((src, dst))
        } else {
            None
        }
    }

    pub(crate) fn get_edge<'id>(&self, src : GraphPtr<'id, NamedNode<N, E>>, dst : GraphPtr<'id, NamedNode<N, E>>) -> Option<Edge<&'_ N, &'_ E>>
    {
        //(E)
        let (this_refs, this) = unsafe {
            let n = &(*src.as_ptr()).internal;
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
        //(E)
        //aliasing check will be required in order to not violate (*) invariants
        let (this_refs, this) = unsafe {
            let n = &mut (*src.as_mut()).internal;
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
}



impl <N, E> GraphRaw<NamedNode<N, E>>
{

    pub(crate) fn get_view<'id>(&self, dst : GraphPtr<'id, NamedNode<N, E>>) -> &node_views::NamedNode<'id, N, E>
    {
        //(E)
        unsafe {
            (*dst.as_ptr()).get_view()
        }
    }

    pub(crate) fn get_view_mut<'id>(&mut self, dst : GraphPtr<'id, NamedNode<N, E>>) -> &mut node_views::NamedNode<'id, N, E>
    {
        //(E)
        unsafe {
            (*dst.as_mut()).get_view_mut()
        }
    }

    pub(crate) fn iter<'a, 'id : 'a>(&'a self, dst : GraphPtr<'id, NamedNode<N, E>>)
        -> impl Iterator<Item = GraphIterRes<Edge<&'a N, &'a E>, GraphPtr<'id, NamedNode<N, E>>>>
    {
        //(E)
        let current = dst.as_ptr();
        let node_refs = unsafe { &(*current).internal.refs };
        self.iter_from_raw(dst, node_refs.iter().map(|x| {
            let p = x.0.as_ptr();
            (p, x.1)
        }))
    }

    pub(crate) fn iter_mut<'a, 'id : 'a>(&'a mut self, src : GraphPtr<'id, NamedNode<N, E>>)
        -> impl Iterator<Item = GraphIterRes<Edge<&'a mut N, &'a mut E>, GraphPtr<'id, NamedNode<N, E>>>>
    {
        //(E)
        let current = src.as_mut();
        //*current is dropped before closure is ever invoked and does not alias
        let node_refs = unsafe { &mut (*current).internal.refs };
        self.iter_mut_from_raw(src, node_refs.iter_mut().map(|x| {
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
