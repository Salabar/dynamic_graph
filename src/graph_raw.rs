#![allow(unused_braces)]

use super::*;

use unsafer::shared_box::*;
use unsafer::pointers::*;
use unsafer::assume::*;

use std::collections::VecDeque;

pub struct GraphItem<E, T> {
    /// Edge data.
    pub values : E,
    /// A pointer to the node.
    pub ptr : T,
}

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

impl <'this, NodeType : 'this> CleanupState<'this, NodeType>
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
        let node = Box::new(NodeType::from_data(data));
        let mut node : SharedBox<_> = node.into();
        let ptr = node.as_ptr();

        unsafe {
            let mut bind = Bind::new();
            let r = bind.get_mut(ptr).meta_mut();
            r.store_index = self.data.len();
            r.cleanup_gen = self.cleanup_gen;
        }

        self.data.push(node);
        ptr
    }

    // Moves internal pointer to the start of the storage, requires item to be a valid pointer
    // This function is used in the preparatory stage of cleanup before any node is dropped therefore (Q W E)
    // apply.
    pub(crate) fn touch(&mut self, frontier : usize, item : *mut NodeType) -> bool
    {
        let mut bind = Bind::new();
        
        let s =  unsafe {
            bind.get_mut(item).meta_mut()
        };

        if s.cleanup_gen != self.cleanup_gen {
            s.cleanup_gen = self.cleanup_gen;
            let item_index = s.store_index;
            s.store_index = frontier;

            let old_frontier = unsafe {
                assume(|| frontier < self.data.len() && item_index < self.data.len());
                let ptr = self.data[frontier].as_ptr();
                bind.get_mut(ptr).meta_mut()
            };

            old_frontier.store_index = item_index;

            self.data.swap(item_index, frontier);
            true
        } else {
            false
        }
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
        let mut bind = Bind::new();
    
        let victim = unsafe {
            bind.get(item)
        };

        let item_index = victim.meta().store_index;

        let last = unsafe {
            let ptr = self.data.last_mut().assume_some().as_ptr();
            bind.get_mut(ptr)
        };

        last.meta_mut().store_index = item_index;

        unsafe {
            //item_index always points to the current position in the Vec
            assume(|| item_index < self.data.len());
        }
        self.data.swap_remove(item_index);
    }

    pub(crate) fn get_edge_raw<E : 'a>(&'a self, src : GraphPtr<'static, NodeType>, dst : GraphPtr<'static, NodeType>, edge : &'a E)
               -> Edge<&'a N, &'a E>
    {
        let this = self.get(src);
        if src == dst {
            Loop(EdgeLoop { this, edge })
        } else {
            let that = unsafe { (*dst.as_ptr()).get() };
            Both(EdgeBoth { this, that, edge })
        }
    }

    pub(crate) fn get_edge_mut_raw<E : 'a>(&'a mut self, src : GraphPtr<'static, NodeType>, dst : GraphPtr<'static, NodeType>, edge : &'a mut E)
               -> Edge<&'a mut N, &'a mut E>
    {
        let this = self.get_mut(src);
        if src == dst {
            Loop(EdgeLoop { this, edge })
        } else {
            //Aliasing checked
            let that = unsafe { (*dst.as_mut()).get_mut() };
            Both(EdgeBoth { this, that, edge })
        }
    }

    pub(crate) fn iter_from_raw<'id : 'a, Iter : 'a, E : 'a>(&'a self, src : GraphPtr<'id, NodeType>, iter : Iter)
               -> impl Iterator<Item = GraphItem<Edge<&'a N, &'a E>, GraphPtr<'id, NodeType>>>
    where Iter : Iterator<Item = (*const NodeType, &'a E)>
    {
        let g = src._guard;
        let current = src.as_ptr();
        iter.map(move |x| {
            let p = x.0;
            let edge = x.1;
            //(W)
            let ptr =  unsafe { GraphPtr::from_ptr(p, g) };
            let that = unsafe { (*p).get() };
        
            if current == p {
                GraphItem { values : Loop(EdgeLoop { this : that, edge }), ptr }
            } else {
                let this = unsafe { (*current).get() };
                GraphItem { values : Both(EdgeBoth { this, that, edge }), ptr }
            }
        })
    }

    pub(crate) fn iter_mut_from_raw<'id : 'a, Iter : 'a, E: 'a>(&'a mut self, src : GraphPtr<'id, NodeType>, iter : Iter)
        -> impl Iterator<Item = GraphItem<Edge<&'a mut N, &'a mut E>, GraphPtr<'id, NodeType>>>
    where Iter : Iterator<Item = (*mut NodeType, &'a mut E)>
    {
        let g = src._guard;
        let current = src.as_mut();
        // (E)
        iter.map(move |x| {
            let p = x.0;
            let edge = x.1;
            //(W)
            let ptr =  unsafe { GraphPtr::from_mut(p, g) };
            let that = unsafe { (*p).get_mut() };

            if current == p {
                GraphItem { values : Loop(EdgeLoop { this : that, edge}), ptr }
            } else {
                //aliasing was explicitly checked
                let this = unsafe { (*current).get_mut() };
                GraphItem { values : Both(EdgeBoth { this, that, edge}), ptr  }
            }
        })
    }

    pub(crate) fn cleanup_precise<'id>(&mut self, root : &impl RootCollection<'id, NodeType>)
    {
        let mut bind = Bind::new();
        self.cleanup_gen.flip();
        let mut state = CleanupState { parent : self, index : 0, queue : VecDeque::new() };
        RootCollection::traverse(root, &mut state);

        while let Some(q) = state.queue.pop_front() {
            unsafe {
                bind.get_mut(q).traverse(&mut state);
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
    pub(crate) fn get_edge<'id>(&self, src : GraphPtr<'id, NamedNode<N, E>>, dst : GraphPtr<'id, NamedNode<N, E>>)
               -> Option<Edge<&'_ N, &'_ E>>
    {
        //(E)
        let src = src.into_static();
        let dst = dst.into_static();

        let src_refs = unsafe { &(*src.as_ptr()).internal.refs };

        src_refs.get(&dst)
                .map(move |e| self.get_edge_raw(src, dst, e))
    }

    pub(crate) fn get_edge_mut<'id>(&mut self, src : GraphPtr<'id, NamedNode<N, E>>, dst : GraphPtr<'id, NamedNode<N, E>>)
               -> Option<Edge<&'_ mut N, &'_ mut E>>
    {
        //(E)
        let src = src.into_static();
        let dst = dst.into_static();

        let src_refs = unsafe { &mut (*src.as_mut()).internal.refs };

        src_refs.get_mut(&dst)
                .map(move |e| self.get_edge_mut_raw(src, dst, e))
    }
}

impl <N, E> GraphRaw<VecNode<N, E>>
{
    pub(crate) fn get_edge<'id>(&self, src : GraphPtr<'id, VecNode<N, E>>, dst : usize)
               -> Option<Edge<&'_ N, &'_ E>>
    {
        //(E)
        let src = src.into_static();
        let src_refs = unsafe { &(*src.as_ptr()).internal.refs };
        src_refs.get(dst)
                .map(move |x| self.get_edge_raw(src, x.0, &x.1))
    }

    pub(crate) fn get_edge_mut<'id>(&mut self, src : GraphPtr<'id, VecNode<N, E>>, dst : usize)
               -> Option<Edge<&'_ mut N, &'_ mut E>>
    {
        //(E)
        let src = src.into_static();
        let src_refs = unsafe { &mut (*src.as_mut()).internal.refs };
        src_refs.get_mut(dst)
                .map(move |x| self.get_edge_mut_raw(src, x.0, &mut x.1))
    }
}

impl <N, E> GraphRaw<OptionNode<N, E>>
{
    pub(crate) fn get_edge<'id>(&self, src : GraphPtr<'id, OptionNode<N, E>>)
               -> Option<Edge<&'_ N, &'_ E>>
    {
        //(E)
        let src = src.into_static();
        let src_refs = unsafe { (*src.as_ptr()).internal.refs.as_ref() };
        src_refs.map(move |x| self
                .get_edge_raw(src, x.0, &x.1))
    }

    pub(crate) fn get_edge_mut<'id>(&mut self, src : GraphPtr<'id, OptionNode<N, E>>)
               -> Option<Edge<&'_ mut N, &'_ mut E>>
    {
        //(E)
        let src = src.into_static();
        let src_refs = unsafe { (*src.as_mut()).internal.refs.as_mut() };
        src_refs.map(move |x| self
                .get_edge_mut_raw(src, x.0, &mut x.1))
    }
}

impl <K, N, E> GraphRaw<TreeNode<K, N, E>> where K : Ord
{
    pub(crate) fn get_edge<'id>(&self, src : GraphPtr<'id, TreeNode<K, N, E>>, dst : &K)
               -> Option<Edge<&'_ N, &'_ E>>
    {
        //(E)
        let src = src.into_static();
        let src_refs = unsafe { &(*src.as_ptr()).internal.refs };
        src_refs.get(dst).map(move |x| self
                .get_edge_raw(src, x.0, &x.1))
    }

    pub(crate) fn get_edge_mut<'id>(&mut self, src : GraphPtr<'id, TreeNode<K, N, E>>, dst : &K)
               -> Option<Edge<&'_ mut N, &'_ mut E>>
    {
        //(E)
        let src = src.into_static();
        let src_refs = unsafe { &mut (*src.as_mut()).internal.refs };
        src_refs.get_mut(dst).map(move |x| self
                .get_edge_mut_raw(src, x.0, &mut x.1))
    }
}

macro_rules! impl_graph_raw {
    ($NodeType:ident, $IterMap:tt, $IterMutMap:tt) => {
        impl <N, E> GraphRaw<$NodeType<N, E>>
        {
            pub(crate) fn bridge<'id>(&mut self, src : GraphPtr<'id, $NodeType<N, E>>,
                                                 dst : GraphPtr<'id, $NodeType<N, E>>)
                -> Option<(&'_ mut node_views::$NodeType<'id, N, E>, &'_ mut node_views::$NodeType<'id, N, E>)>
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

            pub(crate) fn get_view<'id>(&self, dst : GraphPtr<'id, $NodeType<N, E>>) -> &node_views::$NodeType<'id, N, E>
            {
                //(E)
                unsafe {
                    (*dst.as_ptr()).get_view()
                }
            }

            pub(crate) fn get_view_mut<'id>(&mut self, dst : GraphPtr<'id, $NodeType<N, E>>) -> &mut node_views::$NodeType<'id, N, E>
            {
                //(E)
                unsafe {
                    (*dst.as_mut()).get_view_mut()
                }
            }

            pub(crate) fn iter<'a, 'id : 'a>(&'a self, dst : GraphPtr<'id, $NodeType<N, E>>)
                       -> impl Iterator<Item = GraphItem<Edge<&'a N, &'a E>, GraphPtr<'id, $NodeType<N, E>>>>
            {
                //(E)
                let current = dst.as_ptr();
                let node_refs = unsafe { &(*current).internal.refs };
                self.iter_from_raw(dst, node_refs.iter().map($IterMap))
            }

            pub(crate) fn iter_mut<'a, 'id : 'a>(&'a mut self, src : GraphPtr<'id, $NodeType<N, E>>)
                        -> impl Iterator<Item = GraphItem<Edge<&'a mut N, &'a mut E>, GraphPtr<'id, $NodeType<N, E>>>>
            {
                //(E)
                let current = src.as_mut();
                //*current is dropped before closure is ever invoked and does not alias
                let node_refs = unsafe { &mut (*current).internal.refs };
                self.iter_mut_from_raw(src, node_refs.iter_mut().map($IterMutMap))
            }
        }
    }
}

impl_graph_raw!{NamedNode,  {|x| (x.0.as_ptr(),  x.1)}, {|x| (x.0.as_mut(),      x.1)}}
impl_graph_raw!{VecNode,    {|x| (x.0.as_ptr(), &x.1)}, {|x| (x.0.as_mut(), &mut x.1)}}
impl_graph_raw!{OptionNode, {|x| (x.0.as_ptr(), &x.1)}, {|x| (x.0.as_mut(), &mut x.1)}}


impl <K, N, E> GraphRaw<TreeNode<K, N, E>> where K : Ord
{
    pub(crate) fn bridge<'id>(&mut self, src : GraphPtr<'id, TreeNode<K, N, E>>,
                                         dst : GraphPtr<'id, TreeNode<K, N, E>>)
        -> Option<(&'_ mut node_views::TreeNode<'id, K, N, E>, &'_ mut node_views::TreeNode<'id, K, N, E>)>
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

    pub(crate) fn get_view<'id>(&self, dst : GraphPtr<'id, TreeNode<K, N, E>>) -> &node_views::TreeNode<'id, K, N, E>
    {
        //(E)
        unsafe {
            (*dst.as_ptr()).get_view()
        }
    }

    pub(crate) fn get_view_mut<'id>(&mut self, dst : GraphPtr<'id, TreeNode<K, N, E>>) -> &mut node_views::TreeNode<'id, K, N, E>
    {
        //(E)
        unsafe {
            (*dst.as_mut()).get_view_mut()
        }
    }

    pub(crate) fn iter<'a, 'id : 'a>(&'a self, dst : GraphPtr<'id, TreeNode<K, N, E>>)
               -> impl Iterator<Item = GraphItem<Edge<&'a N, &'a E>, GraphPtr<'id, TreeNode<K, N, E>>>>
    {
        //(E)
        let current = dst.as_ptr();
        let node_refs = unsafe { &(*current).internal.refs };
        let iter = node_refs.values().map(|x| (x.0.as_ptr(), &x.1));
        self.iter_from_raw(dst, iter)
    }

    pub(crate) fn iter_mut<'a, 'id : 'a>(&'a mut self, src : GraphPtr<'id, TreeNode<K, N, E>>)
                -> impl Iterator<Item = GraphItem<Edge<&'a mut N, &'a mut E>, GraphPtr<'id, TreeNode<K, N, E>>>>
    {
        //(E)
        let current = src.as_mut();
        //*current is dropped before closure is ever invoked and does not alias
        let node_refs = unsafe { &mut (*current).internal.refs };
        let iter = node_refs.values_mut().map(|x| (x.0.as_mut(), &mut x.1));
        self.iter_mut_from_raw(src, iter)
    }

    
}

impl <T> GraphRaw<T> {
    pub(crate) fn new() -> GraphRaw<T>
    {
        GraphRaw { data : Vec::new(), cleanup_gen : CleanupGen::Even }
    }
}
