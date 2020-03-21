pub use generativity::*;
use std::collections::HashMap;
use core::hash::{Hash, Hasher};
use core::mem::transmute;
use core::ops::{Index, IndexMut, Deref, DerefMut};
use core::ptr::NonNull;
use core::hint::unreachable_unchecked;

/// A checked pointer type used to access and traverse graph nodes in the crate. This pointer cannot be dereferenced
/// and requires the parent anchor object to access the data stored in the collection.
#[repr(transparent)]
pub struct GraphPtr<'id, T> {
    node : NonNull<T>,
    _guard : Id<'id>
}

impl <'id, T> PartialEq for GraphPtr<'id, T> {
    fn eq(&self, other : &Self) -> bool
    {
        self.node == other.node
    }
}

impl <'id, T> Eq for GraphPtr<'id, T> {}


impl <'id, T> GraphPtr<'id, T> {
    fn as_mut(self) -> *mut T
    {
        self.node.as_ptr()
    }

    /// Returns a raw pointer to the graph node. This pointer should not be dereferenced directly and is meant
    /// to be a way to cache GraphPtrs between cleanups. You must ensure the node behind this pointer
    /// will not be deleted when the parent anchor is dropped
    pub fn as_ptr(self) -> *const T
    {
        self.node.as_ptr() as *const T
    }

    //ptr must be a valid pointer.
    //node behind ptr must belong to the same graph as an 'id branded anchor.
    unsafe fn from_mut(ptr : *mut T, guard : Id<'id>) -> Self
    {
        GraphPtr { node : NonNull::new_unchecked(ptr), _guard : guard }
    }

    unsafe fn from_ptr(ptr : *const T, guard : Id<'id>) -> Self
    {
        GraphPtr { node : NonNull::new_unchecked(ptr as *mut T), _guard : guard }
    }

    unsafe fn make_static(self) -> GraphPtr<'static, T>
    {
        transmute(self)
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


/* TODO
#[derive(PartialEq, Eq)]
pub struct GenericNode<N, T> {
    refs : T,
    data : N,
}
pub type NamedNode<N, E> = GenericNode<N, HashMap<*const NamedNode<N, E>, E>>;
*/

/// A node type which uses node pointers as keys in the edge collection.
#[derive(PartialEq, Eq)]
#[repr(C)]
pub struct NamedNode<N, E> {
    refs : HashMap<GraphPtr<'static, NamedNode<N, E>>, E>,
    data : N,
    service : ServiceData,
}

#[derive(PartialEq, Eq, Clone, Copy)]
enum CleanupGen {
    Even, Odd
}

#[derive(PartialEq, Eq, Clone, Copy)]
pub struct ServiceData {
    cleanupGen : CleanupGen,
    storeIndex: usize,
}

pub mod node_views {
    use super::*;

    #[repr(C)]
    pub struct NamedNode<'id, N, E> {
        pub refs : HashMap<GraphPtr<'id, super::NamedNode<N, E>>, E>,
        pub data : N,
    }
}

pub trait GraphNode : Sized {
    type Node;
    type Edge;
    fn get(&self) -> &Self::Node;
    fn get_mut(&mut self) -> &mut Self::Node;

    fn service(&self) -> &ServiceData;
    fn service_mut(&mut self) -> &mut ServiceData;

    fn from_data(data : Self::Node) -> Self;
}

impl <N, E> GraphNode for NamedNode<N, E> {
    type Node = N;
    type Edge = E;

    fn get(&self) -> &Self::Node
    {
        &self.data
    }

    fn get_mut(&mut self) -> &mut Self::Node
    {
        &mut self.data
    }

    fn from_data(data : Self::Node) -> Self
    {
        let service = ServiceData { cleanupGen : CleanupGen::Even, storeIndex : 0 };
        NamedNode { refs : HashMap::new(), data, service }
    }

    fn service(&self) -> &ServiceData {
        &self.service
    }
    
    fn service_mut(&mut self) -> &mut ServiceData {
        &mut self.service
    }

}

pub struct GraphIterRes<E, T> {
    pub values : E,
    pub ptr : T,
}

pub struct EdgeBoth<N, E> {
    ///Value from the source node.
    pub this : N,
    ///Value from the destination node.
    pub that : N,
    ///Value from the edge.
    pub edge : E
}

pub struct EdgeSingle<N, E> {
    ///Value from the node.
    pub this : N,
    ///Value from the edge.
    pub edge : E
}
///View into nodes data connected by an edge. Both if the edge connects two different nodes and Loop if the edge loops back to the source node.
pub enum Edge<N, E> {
    Both(EdgeBoth<N, E>),
    Loop(EdgeSingle<N, E>),
}

pub use crate::Edge::Both;
pub use crate::Edge::Loop;

impl <N, E> Edge<N, E> {
    ///Returns data from the source node and the edge.
    pub fn this(self) -> EdgeSingle<N, E>
    {
        match self {
            Both(s) => EdgeSingle { this : s.this, edge : s.edge },
            Loop(s) => s,
        }
    }

    ///Returns data from the destination node and the edge.
    pub fn that(self) -> EdgeSingle<N, E>
    {
        match self {
            Both(s) => EdgeSingle { this : s.that, edge : s.edge },
            Loop(s) => s,
        }
    }

    ///Returns data from both nodes and the edge. Panics if self is a Loop.
    pub fn unwrap(self) -> EdgeBoth<N, E>
    {
        match self {
            Both(s) => s,
            _ => panic!("called `Edge::unwrap()` on a `Loop` value"),
        }
    }

    /// Returns data from both nodes and the edge. Undefined behavior if self is a Loop.
    /// # Safety
    /// Caller must guarantee value of self to be Both.
    pub unsafe fn unwrap_unchecked(self) -> EdgeBoth<N, E>
    {
        match self {
            Both(s) => s,
            _ => unreachable_unchecked(),
        }
    }
}

#[derive(PartialEq, Eq)]
pub struct GraphRaw<T> {
    data : Vec<Box<T>>,
    cleanupGen : CleanupGen,
}

impl <'a, N : 'a, E : 'a, NodeType> GraphRaw<NodeType>
where NodeType : GraphNode<Node = N, Edge = E>
{
    fn spawn_detached(&mut self, data : N) -> *const NodeType
    {
        let mut node = Box::new(NodeType::from_data(data));
        node.service_mut().storeIndex = self.data.len();
        node.service_mut().cleanupGen = self.cleanupGen;
        let ptr : *const NodeType = &*node;
        self.data.push(node);
        ptr
    }

    //GraphPtr here and later never dangles because there is no safe way to create
    //one after anchor branded with the same 'id is dropped and there is no safe way to dispose of the nodes
    //before it happens
    //Every reference bound to &self is protected from aliasing due to Rust borrowing rules
    fn get<'id>(&self, item : GraphPtr<'id, NodeType>) -> &N
    {
        unsafe {
            (*item.as_ptr()).get()
        }
    }

    fn get_mut<'id>(&mut self, item : GraphPtr<'id, NodeType>) -> &mut N
    {
        unsafe {
            (*item.as_mut()).get_mut()
        }
    }

    unsafe fn kill(&mut self, item : *const NodeType)
    {
        let storeIndex = {
            (*item).service().storeIndex
        };
        
        if self.data.len() > 0 && storeIndex < self.data.len() {
            self.data.last_mut().unwrap().service_mut().storeIndex = storeIndex;
            self.data.swap_remove(storeIndex);
        } else { 
            //storeIndex always points to the current position in the Vec
            unreachable!()
            //unreachable_unchecked()
        }
    }

    fn iter_from_raw<'id : 'a, Iter : 'a>(&'a self, src : GraphPtr<'id, NodeType>, iter : Iter)
        ->impl Iterator<Item = GraphIterRes<Edge<&'a N, &'a E>, GraphPtr<'id, NodeType>>>
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

    fn iter_mut_from_raw<'id : 'a, Iter : 'a>(&'a mut self, src : GraphPtr<'id, NodeType>, iter : Iter)
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
    fn bridge<'id>(&mut self, src : GraphPtr<'id, node_views::NamedNode<'id, N, E>>,
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
}

impl <N, E> GraphRaw<NamedNode<N, E>> {
    fn get_edge<'id>(&self, src : GraphPtr<'id, NamedNode<N, E>>, dst : GraphPtr<'id, NamedNode<N, E>>) -> Option<Edge<&'_ N, &'_ E>>
    {
        let this = unsafe { &(*src.as_ptr()) };

        let this_refs = &this.refs;
        let this = &this.data;

        let s_dst = unsafe { dst.make_static() };
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

    fn get_edge_mut<'id>(&mut self, src : GraphPtr<'id, NamedNode<N, E>>, dst : GraphPtr<'id, NamedNode<N, E>>) -> Option<Edge<&'_ mut N, &'_ mut E>>
    {
        //aliasing check will be required in order to not violate (*) invariants
        let this = unsafe { &mut (*src.as_mut()) };

        let this_refs = &mut this.refs;
        let this = &mut this.data;
        
        let s_dst = unsafe { dst.make_static() };
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

    fn get_view<'id>(&self, dst : GraphPtr<'id, NamedNode<N, E>>) -> &node_views::NamedNode<'id, N, E>
    {
        unsafe {
            transmute(&*dst.as_ptr())
        }
    }

    fn get_view_mut<'id>(&mut self, dst : GraphPtr<'id, NamedNode<N, E>>) -> &mut node_views::NamedNode<'id, N, E>
    {
        unsafe {
            transmute(&mut *dst.as_mut())
        }
    }
}

impl <T> GraphRaw<T> {
    fn new() -> GraphRaw<T>
    {
        GraphRaw { data : Vec::new(), cleanupGen : CleanupGen::Even }
    }
}

pub struct GenericGraph<Root, NodeType> {
    internal : GraphRaw<NodeType>,
    root : Root
}

impl <Root : Default, NodeType> Default for GenericGraph<Root, NodeType> {
    fn default() -> Self
    {
        GenericGraph::new()
    }
}

impl <Root : Default, NodeType> GenericGraph<Root, NodeType> {
    pub fn new() -> Self
    {
        GenericGraph { root : Root::default(), internal : GraphRaw::new() }
    }
}

impl <Root, NodeType> GenericGraph<Root, NodeType> {
    /// Creates an AnchorMut from a generativity brand using selected cleanup strategy.
    /// Prefer make_anchor_mut macro in application code.
    /// # Safety
    /// Caller must use a unique `guard` from generativity::Guard.
    pub unsafe fn anchor_mut<'id>(&mut self, guard : Id<'id>, strategy : CleanupStrategy)
                                  -> AnchorMut<'_, 'id, GenericGraph<Root, NodeType>>
    {
        AnchorMut { parent : self, _guard : guard, strategy }
    }
}

pub type VecGraph<T> = GenericGraph<Vec<GraphPtr<'static, T>>, T>;

/// A strategy AnchorMut employs to perform cleanup after drop.
pub enum CleanupStrategy {
    /// AnchorMut never cleans up.
    Never,
}

pub struct AnchorMut<'this, 'id : 'this, T : 'this> {
    parent: &'this mut T,
    strategy : CleanupStrategy,
    _guard : Id<'id>,
}

impl <'this, 'id: 'this, T : 'this> Drop for AnchorMut<'this, 'id, T>
{
    fn drop(&mut self) {}
}

impl <'this, 'id : 'this, N : 'this, E : 'this, Root : 'this>
Index<GraphPtr<'id, NamedNode<N, E>>>
for AnchorMut<'this, 'id, GenericGraph<Root, NamedNode<N, E>>>
{
    type Output = node_views::NamedNode<'id, N, E>;
    fn index(&self, dst : GraphPtr<'id, NamedNode<N, E>>) -> &Self::Output
    {
        self.internal().get_view(dst)
    }
}

impl <'this, 'id : 'this, N : 'this, E : 'this, Root : 'this>
IndexMut<GraphPtr<'id, NamedNode<N, E>>>
for AnchorMut<'this, 'id, GenericGraph<Root, NamedNode<N, E>>>
{
    fn index_mut(&mut self, dst : GraphPtr<'id, NamedNode<N, E>>) -> &mut Self::Output {
        self.internal_mut().get_view_mut(dst)
    }
}

#[macro_export]
/// Creates an AnchorMut using selected cleanup strategy.
macro_rules! make_anchor_mut
{
    ($name:ident, $parent:ident, $strategy:ident) => {
        make_guard!(g);
        let mut $name = unsafe { $parent.anchor_mut(Id::from(g), $crate::CleanupStrategy::$strategy) };
    };
}

impl <'this, 'id : 'this, N : 'this, E : 'this, NodeType : 'this, Root : 'this>
AnchorMut<'this, 'id, GenericGraph<Root, NodeType>>
where NodeType : GraphNode<Node = N, Edge = E>
{
    fn internal(&self) -> &GraphRaw<NodeType> {
        &self.parent.internal
    }
    /// Creates a checked pointer from a raw pointer
    ///# Safety
    /// Caller must guarantee `raw` points to a node which was not cleaned up and belongs to the parent graph 
    pub unsafe fn from_raw(&self, raw : *const NodeType) -> GraphPtr<'id, NodeType>
    {
        GraphPtr::from_ptr(raw, self._guard)
    }
    /// Creates an immutable cursor pointing to `dst`
    pub fn cursor(&self, dst : GraphPtr<'id, NodeType>) -> Cursor<'_, 'id, NodeType>
    {
        Cursor { parent : self.internal(), current : dst }
    }

    fn root_iter_from_raw<'a, Iter : 'a>(&'a self, iter : Iter)
        -> impl Iterator<Item = (&'a N, GraphPtr<'id, NodeType>)>
        where Iter : Iterator<Item = *const NodeType>
    {
        let g = self._guard;
        iter.map(move |x| {
            let p =  unsafe { GraphPtr::from_ptr(x, g) };
            let data = unsafe { (*x).get() };
            (data, p)
        })
    }
}

impl <'this, 'id : 'this, N : 'this, E : 'this, NodeType : 'this, Root : 'this>
AnchorMut<'this, 'id, GenericGraph<Root, NodeType>>
where NodeType : GraphNode<Node = N, Edge = E>
{
    fn internal_mut(&mut self) -> &mut GraphRaw<NodeType>
    {
        &mut self.parent.internal
    }

    /// Allocates a new node and returns the pointer. This node will become inaccessible when parent anchor
    /// is dropped and will be disposed of upon next cleanup unless you attach it to root or another node accessible
    /// from the root.
    pub fn spawn(&mut self, data : N) -> GraphPtr<'id, NodeType>
    {
        let ptr = self.internal_mut().spawn_detached(data);
        unsafe {
            GraphPtr::from_ptr(ptr, self._guard )
        }
    }

    /// Creates a mutable cursor pointing to `dst`.
    pub fn cursor_mut(&mut self, dst : GraphPtr<'id, NodeType>)
           -> CursorMut<'_, 'id, NodeType>
    {
        CursorMut { parent : self.internal_mut(), current : dst }
    }


    fn root_iter_mut_from_raw<'a, Iter : 'a>(&'a mut self, iter : Iter)
        -> impl Iterator<Item = (&'a mut N, GraphPtr<'id, NodeType>)>
        where Iter : Iterator<Item = *mut NodeType>
    {
        let g = self._guard;
        iter.map(move |x| {
            let p =  unsafe { GraphPtr::from_mut(x, g) };
            let data = unsafe { (*x).get_mut() };
            (data, p)
        })
    }
}


impl <'this, 'id : 'this, N : 'this, E : 'this, NodeType : 'this>
AnchorMut<'this, 'id, VecGraph<NodeType>>
where NodeType : GraphNode<Node = N, Edge = E>
{
    /// Allocates a new node and returns the pointer. Attaches the node to the root.
    pub fn spawn_attached(&mut self, data : N) -> GraphPtr<'id, NodeType>
    {
        let res = self.internal_mut().spawn_detached(data);
        let res = unsafe { self.from_raw(res)};
        let a = unsafe { (res.make_static()) };
        self.parent.root.push(a);
        res
    }

    /// Provides safe direct access to the collection of the root.
    pub fn root(&self) -> &Vec<GraphPtr<'id, NodeType>>
    {
        unsafe {
            transmute(&self.parent.root)
        }
    }

    /// Provides safe mutable direct access to the collection of the root.
    pub fn root_mut(&mut self) -> &mut Vec<GraphPtr<'id, NodeType>>
    {
        unsafe {
            transmute(&mut self.parent.root)
        }
    }

    /// Returns an iterator over datas and node pointers attached to the root.
    pub fn iter(&self) -> impl Iterator<Item = (&'_ N, GraphPtr<'id, NodeType>)>
    {
        self.root_iter_from_raw(self.parent.root.iter().map(|x| { x.as_ptr() }))
    }

    /// Returns a mutable iterator over datas and node pointers attached to the root.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = (&'_ mut N, GraphPtr<'id, NodeType>)>
    {
        //unbind reference. root_iter_mut_from_raw only uses _guard and not root
        let r : &mut Vec<GraphPtr<NodeType>> = unsafe { transmute(&mut self.parent.root) };
        self.root_iter_mut_from_raw(r.iter_mut().map(|x| { x.as_mut() }))
    }
}

pub struct CursorMut<'this, 'id : 'this, T : 'this> {
    parent : &'this mut GraphRaw<T>,
    current : GraphPtr<'id, T>
}

pub struct Cursor<'this, 'id : 'this, T : 'this> {
    parent : &'this GraphRaw<T>,
    current : GraphPtr<'id, T>
}

macro_rules! impl_cursor_immutable {
    ($cursor_type:ident) => {
        impl <'this, 'id : 'this, N : 'this, E : 'this, NodeType : 'this>
        $cursor_type<'this, 'id, NodeType>
        where NodeType : GraphNode<Node = N, Edge = E>
        {
            /// Returns a pointer to the current node the cursor points to.
            pub fn at(&self) -> GraphPtr<'id, NodeType>
            {
                self.current
            }
        
            /// Returns true if the cursor points to `dst`.
            pub fn is_at(&self, dst : GraphPtr<'id, NodeType>) -> bool
            {
                dst == self.at()
            }
        
            /// Makes the cursor point to the `dst`.
            pub fn jump(&mut self, dst : GraphPtr<'id, NodeType>)
            {
                self.current = dst;
            }
        }
        
        impl <'this, 'id : 'this, N : 'this, E : 'this>
        $cursor_type<'this, 'id, NamedNode<N, E>>
        {
            /// Returns an iterator over edges and node pointers attached to the current node.
            pub fn iter(&self) -> impl Iterator<Item = GraphIterRes<Edge<&'_ N, &'_ E>, GraphPtr<'id, NamedNode<N, E>>>>
            {
                let current = self.current;
                let node_refs = unsafe { &(*current.as_ptr()).refs };
                self.parent.iter_from_raw(self.current, node_refs.iter().map(|x|{
                    let p = x.0.as_ptr();
                    (p, x.1)
                }))
            }
        
            /// Returns Some if `dst` is attached to the current node and None otherwise.
            pub fn get_edge(&self, dst : GraphPtr<'id, NamedNode<N, E>>) -> Option<Edge<&'_ N, &'_ E>>
            {
                self.parent.get_edge(self.at(), dst)
            }
        }
        
        impl <'this, 'id : 'this, N : 'this, E : 'this> Deref for $cursor_type<'this, 'id, NamedNode<N, E>>
        {
            type Target = node_views::NamedNode<'id, N, E>;
            fn deref(&self) -> &Self::Target
            {
                self.parent.get_view(self.at())
            }
        }
    }
}

impl_cursor_immutable!{CursorMut}
impl_cursor_immutable!{Cursor}

impl <'this, 'id : 'this, N : 'this, E : 'this>
CursorMut<'this, 'id, NamedNode<N, E>>
{
    /// Returns a mutable iterator over edges and node pointers attached to the current node.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = GraphIterRes<Edge<&'_ mut N, &'_ mut E>, GraphPtr<'id, NamedNode<N, E>>>>
    {
        let current = self.current;
        //*current is dropped before closure is ever invoked and does not alias
        let node_refs = unsafe { &mut (*current.as_mut()).refs };
        self.parent.iter_mut_from_raw(current, node_refs.iter_mut().map(|x| {
            let p = x.0.as_mut();
            (p, x.1)
        }))
    }

    /// Returns Some if `dst` is attached to the current node and None otherwise.
    fn get_edge_mut(&mut self, dst : GraphPtr<'id, NamedNode<N, E>>) -> Option<Edge<&'_ mut N, &'_ mut E>>
    {
        self.parent.get_edge_mut(self.at(), dst)
    }
}

impl <'this, 'id : 'this, N : 'this, E : 'this> DerefMut for CursorMut<'this, 'id, NamedNode<N, E>>
{
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.parent.get_view_mut(self.at())
    }
}
