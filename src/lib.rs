pub use generativity::*;
use std::collections::HashMap;
use core::hash::{Hash, Hasher};
use core::mem::transmute;
use core::ops::{Index, IndexMut, Deref, DerefMut};
use core::ptr::NonNull;
use core::hint::unreachable_unchecked;

/// A checked pointer type used to access and traverse graph nodes in the crate. This pointer cannot be dereferenced
/// directly and requires an anchor object to access the data stored in the collection. Thanks to 'generativity'
/// crate this pointer can only be used with and created from an anchor branded with the same `'id` and cannot
/// be used once parent anchor is dropped.
#[repr(transparent)]
#[derive(Eq)]
pub struct GraphPtr<'id, T> {
    node : NonNull<T>,
    _guard : Id<'id>
}

impl <'id, T> PartialEq for GraphPtr<'id, T> {
    fn eq(&self, other : &Self) -> bool {
        self.node == other.node
    }
}

impl <'id, T> GraphPtr<'id, T> {
    fn as_mut(self) -> *mut T {
        self.node.as_ptr()
    }

    /// Returns a raw pointer to the graph node. This pointer should not be dereferenced directly and is meant
    /// to be a way to cache GraphPtrs between cleanups. You must ensure the node behind this pointer
    /// will not be deleted when the parent anchor is dropped
    pub fn as_ptr(self) -> *const T {
        self.node.as_ptr() as *const T
    }

    //ptr must be a valid pointer.
    //node behind ptr must belong to the same graph as an 'id branded anchor.
    unsafe fn from_mut(ptr : *mut T, guard : Id<'id>) -> Self {
        GraphPtr { node : NonNull::new_unchecked(ptr), _guard : guard }
    }

    unsafe fn from_ptr(ptr : *const T, guard : Id<'id>) -> Self {
        GraphPtr { node : NonNull::new_unchecked(ptr as *mut T), _guard : guard }
    }
}

impl <'id, T> Hash for GraphPtr<'id, T>  {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.node.hash(state);
    }
}

impl <'id, T> Clone for GraphPtr<'id, T> {
    fn clone(&self) -> GraphPtr<'id, T> {
        GraphPtr { node : self.node, _guard : self._guard }
    }
}
impl <'id, T> Copy for GraphPtr<'id, T> {}


/* TODO
#[derive(PartialEq, Eq)]
pub struct GenericNode<N, T> {
    refs : T,
    payload : N,
}
pub type NamedNode<N, E> = GenericNode<N, HashMap<*const NamedNode<N, E>, E>>;
*/

/// A node type which uses node pointers as keys in the edge collection.
#[derive(PartialEq, Eq)]
#[repr(C)]
pub struct NamedNode<N, E> {
    refs : HashMap<*const NamedNode<N, E>, E>,
    payload : N,
}

/// Part of the experimental API which allows a user to modify the contents of the collection directly.
/// This API relies on undefined behavior and should not be used.
pub mod node_views {
    use super::*;

    #[repr(C)]
    pub struct NamedNode<'id, N, E> {
        pub refs : HashMap<GraphPtr<'id, super::NamedNode<N, E>>, E>,
        pub payload : N,
    }
}

pub trait GraphNode : Sized {
    type Node;
    type Edge;
    fn get(&self) -> &Self::Node;
    fn get_mut(&mut self) -> &mut Self::Node;
    fn from_payload(data : Self::Node) -> Self;
}

impl <N, E> GraphNode for NamedNode<N, E> {
    type Node = N;
    type Edge = E;

    fn get(&self) -> &Self::Node {
        &self.payload
    }

    fn get_mut(&mut self) -> &mut Self::Node {
        &mut self.payload
    }

    fn from_payload(data : Self::Node) -> Self {
        NamedNode { refs : HashMap::new(), payload : data }
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
///View into two nodes connected by an edge. Takes into account the case when both nodes are the same.
pub enum Edge<N, E> {
    Both(EdgeBoth<N, E>),
    Loop(EdgeSingle<N, E>),
}

pub use crate::Edge::Both;
pub use crate::Edge::Loop;

impl <N, E> Edge<N, E> {
    ///Returns data from the source node and the edge.
    pub fn this(self) -> EdgeSingle<N, E> {
        match self {
            Both(s) => EdgeSingle { this : s.this, edge : s.edge },
            Loop(s) => s,
        }
    }

    ///Returns data from the destination node and the edge.
    pub fn that(self) -> EdgeSingle<N, E> {
        match self {
            Both(s) => EdgeSingle { this : s.that, edge : s.edge },
            Loop(s) => s,
        }
    }

    ///Returns data from both nodes and the edge. Panics if self is a Loop.
    pub fn unwrap(self) -> EdgeBoth<N, E> {
        match self {
            Both(s) => s,
            _ => panic!("called `Edge::unwrap()` on a `Loop` value"),
        }
    }

    /// Returns data from both nodes and the edge. Undefined behavior if self is a Loop.
    /// # Safety
    /// Caller must guarantee value of self to be Both
    pub unsafe fn unwrap_unchecked(self) -> EdgeBoth<N, E> {
        match self {
            Both(s) => s,
            _ => unreachable_unchecked(),
        }
    }
}

#[derive(PartialEq, Eq)]
pub struct GraphRaw<T> {
    data : Vec<Box<T>>,
}

impl <N, E, NodeType> GraphRaw<NodeType>
where NodeType : GraphNode<Node = N, Edge = E>
{
    fn spawn_detached(&mut self, payload : N) -> *const NodeType {
        let node = Box::new(NodeType::from_payload(payload));
        let ptr : *const NodeType = &*node;
        self.data.push(node);
        ptr
    }

    //GraphPtr here and later never dangles because there is no safe way to create
    //one after anchor branded with the same 'id is dropped and there is no safe way to dispose of the nodes
    //before it happens
    //Every reference bound to &self is protected from aliasing due to Rust borrowing rules
    fn get<'id>(&self, item : GraphPtr<'id, NodeType>) -> &N {
        unsafe {
            (*item.as_ptr()).get()
        }
    }

    fn get_mut<'id>(&mut self, item : GraphPtr<'id, NodeType>) -> &mut N {
        unsafe {
            (*item.as_mut()).get_mut()
        }
    }

    #[allow(dead_code)]
    #[allow(unused_variables)]
    unsafe fn kill(&mut self, item : *const NodeType) {
        unimplemented!();
    }
}

impl <N, E, NodeType> GraphRaw<NodeType>
where NodeType : GraphNode<Node = N, Edge = E>
{
    fn bridge<'id>(&mut self, src : GraphPtr<'id, NodeType>, dst : GraphPtr<'id, NodeType>) -> Edge<&'_ mut N, ()> {
        let this = self.get_mut(src);
        if src == dst { 
            Loop(EdgeSingle { this, edge : () })
        } else {
            //aliasing was explicitely checked
            let that = unsafe { (*dst.as_mut()).get_mut() };
            Both(EdgeBoth { this, that, edge : () })
        }
    }
}

impl <N, E> GraphRaw<NamedNode<N, E>> {
    fn connect<'id>(&mut self, src : GraphPtr<'id, NamedNode<N, E>>, dst : GraphPtr<'id, NamedNode<N, E>>, edge : E) {
        let refs = unsafe { &mut (*src.as_mut()).refs };
        refs.insert(dst.as_ptr(), edge);
    }

    fn disconnect<'id>(&mut self, src : GraphPtr<'id, NamedNode<N, E>>, dst : GraphPtr<'id, NamedNode<N, E>>) {
        let refs = unsafe { &mut (*src.as_mut()).refs };
        refs.remove(&dst.as_ptr());
    }

    fn get_edge<'id>(&self, src : GraphPtr<'id, NamedNode<N, E>>, dst : GraphPtr<'id, NamedNode<N, E>>) -> Option<Edge<&'_ N, &'_ E>> {
        let this = unsafe { &(*src.as_ptr()) };

        let this_refs = &this.refs;
        let this = &this.payload;
        
        if let Some(e) = this_refs.get(&dst.as_ptr()) {
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

    fn get_edge_mut<'id>(&mut self, src : GraphPtr<'id, NamedNode<N, E>>, dst : GraphPtr<'id, NamedNode<N, E>>) -> Option<Edge<&'_ mut N, &'_ mut E>> {
        //aliasing check will be required in order to not violate (*) invariants
        let this = unsafe { &mut (*src.as_mut()) };

        let this_refs = &mut this.refs;
        let this = &mut this.payload;
        
        if let Some(e) = this_refs.get_mut(&(dst.as_ptr())) {
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
}

impl <T> GraphRaw<T> {
    fn new() -> GraphRaw<T> {
        GraphRaw { data : Vec::new() }
    }
}

pub struct GenericGraph<Root, NodeType> {
    internal : GraphRaw<NodeType>,
    root : Root
}

pub type VecGraph<T> = GenericGraph<Vec<*const T>, T>;

impl <Root : Default, NodeType> Default for GenericGraph<Root, NodeType> {
    fn default() -> Self {
        GenericGraph::new()
    }
}

impl <Root : Default, NodeType> GenericGraph<Root, NodeType> {
    pub fn new() -> Self {
        GenericGraph { root : Root::default(), internal : GraphRaw::new() }
    }
}

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


impl <'this, 'id: 'this, T : 'this> Drop for AnchorMut<'this, 'id, T> {
    fn drop(&mut self) {}
}

impl <'this, 'id : 'this, N : 'this, E : 'this, NodeType : 'this, Root : 'this>
Index<GraphPtr<'id, NodeType>> for AnchorMut<'this, 'id, GenericGraph<Root, NodeType>>
where NodeType : GraphNode<Node = N, Edge = E>
{
    type Output = N;
    fn index(&self, dst : GraphPtr<'id, NodeType>) -> &Self::Output {
        &self.internal().get(dst)
    }
}

impl <'this, 'id : 'this, N : 'this, E : 'this, NodeType : 'this, Root : 'this>
IndexMut<GraphPtr<'id, NodeType>> for AnchorMut<'this, 'id, GenericGraph<Root, NodeType>>
where NodeType : GraphNode<Node = N, Edge = E>
{
    fn index_mut(&mut self, dst : GraphPtr<'id, NodeType>) -> &mut Self::Output {
        self.internal_mut().get_mut(dst)
    }
}

impl <Root, NodeType> GenericGraph<Root, NodeType> {
    /// Creates an AnchorMut from a generativity brand using selected cleanup strategy.
    /// Prefer make_anchor_mut macro in application code.
    /// # Safety
    /// Caller must use a unique `guard` from generativity::Guard.
    pub unsafe fn anchor_mut<'id>(&mut self, guard : Id<'id>, strategy : CleanupStrategy)
                                  -> AnchorMut<'_, 'id, GenericGraph<Root,NodeType>> {
        AnchorMut { parent : self, _guard : guard, strategy }
    }
}

#[macro_export]
/// Creates an AnchorMut using a selected cleanup strategy.
macro_rules! make_anchor_mut {
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
    pub unsafe fn ref_from_ptr(&self, raw : *const NodeType) -> GraphPtr<'id, NodeType> {
        GraphPtr::from_ptr(raw, self._guard)
    }
    /// Creates an immutable cursor pointing to `dst`
    pub fn cursor(&self, dst : GraphPtr<'id, NodeType>) -> Cursor<'_, 'id, NodeType> {
        Cursor { parent : self.internal(), current : dst }
    }
}

impl <'this, 'id : 'this, N : 'this, E : 'this, NodeType : 'this, Root : 'this>
AnchorMut<'this, 'id, GenericGraph<Root, NodeType>>
where NodeType : GraphNode<Node = N, Edge = E>
{
    fn internal_mut(&mut self) -> &mut GraphRaw<NodeType> {
        &mut self.parent.internal
    }

    /// Allocates a new node and returns the pointer. This node will become inaccessible when parent anchor
    /// is dropped and will be disposed of upon next cleanup unless you attach it to another node accessible
    /// from the root.
    pub fn spawn_detached(&mut self, payload : N) -> GraphPtr<'id, NodeType> {
        let ptr = self.internal_mut().spawn_detached(payload);
        unsafe {
            GraphPtr::from_ptr(ptr, self._guard )
        }
    }

    /// Creates a mutable cursor pointing to `dst`.
    pub fn cursor_mut(&mut self, dst : GraphPtr<'id, NodeType>) ->
                      CursorMut<'_, 'id, NodeType> {
        CursorMut { parent : self.internal_mut(), current : dst }
    }
}

impl <'this, 'id : 'this, N : 'this, E : 'this, Root : 'this>
AnchorMut<'this, 'id, GenericGraph<Root, NamedNode<N, E>>>
{
    /// Creates an edge from `src` to `dst` with the value `edge`.
    pub fn connect(&mut self, src : GraphPtr<'id, NamedNode<N, E>>,
                   dst : GraphPtr<'id, NamedNode<N, E>>, edge : E)
    {
        self.internal_mut().connect(src, dst, edge);
    }

    /// Removes an edge from `src` to `dst`.
    pub fn disconnect(&mut self, src : GraphPtr<'id, NamedNode<N, E>>,
                      dst : GraphPtr<'id, NamedNode<N, E>>)
    {
        self.internal_mut().disconnect(src, dst);
    }

    /// Provides direct access to the contents of the `dst` node.
    /// !!! Relies on undefined behavior and should not be used !!!
    pub fn view(&self, dst : GraphPtr<'id, NamedNode<N, E>>) -> &node_views::NamedNode<'id, N, E> {
        // not sound, but should be
        let this = unsafe { &(*dst.as_ptr()) };
        unsafe { transmute(this) }
    }

    /// Provides mutable direct access to the contents of the `dst` node.
    /// !!! Relies on undefined behavior and should not be used !!!
    pub fn view_mut(&mut self, dst : GraphPtr<'id, NamedNode<N, E>>) -> &mut node_views::NamedNode<'id, N, E> {
        // not sound, but should be
        let this = unsafe { &mut (*dst.as_mut()) };
        unsafe { transmute(this) }
    }
}

impl <'this, 'id : 'this, N : 'this, E : 'this, NodeType : 'this>
AnchorMut<'this, 'id, VecGraph<NodeType>>
where NodeType : GraphNode<Node = N, Edge = E>
{
    /// Allocates a new node and returns the pointer. Attaches the node to the root.
    pub fn spawn(&mut self, payload : N) -> GraphPtr<'id, NodeType> {
        let res = self.spawn_detached(payload);
        self.attach(res);
        res
    }

    /// Provides safe direct access to the collection of the root.
    /// !!! Relies on undefined behavior and should not be used !!!
    pub fn root(&self) -> &Vec<GraphPtr<'id, NodeType>> {
        // not sound, but should be
        unsafe {
            transmute(&self.parent.root)
        }
    }

    /// Provides safe mutable direct access to the collection of the root.
    /// !!! Relies on undefined behavior and should not be used !!!
    pub fn root_mut(&mut self) -> &mut Vec<GraphPtr<'id, NodeType>> {
        // not sound, but should be
        unsafe {
            transmute(&mut self.parent.root)
        }
    }

    /// Attaches `dst` to the root using Vec::push.
    pub fn attach(&mut self, dst : GraphPtr<'id, NodeType>) {
        self.parent.root.push(dst.as_ptr());
    }

    /// Detaches a node from the root using Vec::swap_remove.
    pub fn detach(&mut self, index : usize) {
        self.parent.root.swap_remove(index);
    }

    /// Returns an iterator over payloads and node pointers attached to the root.
    pub fn iter(&self) -> impl Iterator<Item = (&'_ N, GraphPtr<'id, NodeType>)> {
        let g = self._guard;
        self.parent.root.iter().map(move |x| {
            let x = *x;
            let p =  unsafe { GraphPtr::from_ptr(x, g) };
            let payload = unsafe { (*x).get() };
            (payload, p)
        })
    }

    /// Returns a mutable iterator over payloads and node pointers attached to the root.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = (&'_ mut N, GraphPtr<'id, NodeType>)> {
        let g = self._guard;
        self.parent.root.iter_mut().map(move |x| {
            let x = *x as *mut NodeType;
            let p =  unsafe { GraphPtr::from_mut(x, g) };
            //this won't alias since each next() will invalidate the previous one.
            let payload = unsafe { (*x).get_mut() };
            (payload, p)
        })
    }
}

pub struct CursorMut<'this, 'id : 'this, T : 'this> {
    parent : &'this mut GraphRaw<T>,
    current : GraphPtr<'id, T>
}

#[derive(Clone, Copy, PartialEq, Eq)]
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
            pub fn at(&self) -> GraphPtr<'id, NodeType> {
                self.current
            }
        
            /// Returns true if the cursor points to `dst`.
            pub fn is_at(&self, dst : GraphPtr<'id, NodeType>) -> bool {
                dst == self.at()
            }
        
            /// Returns a reference to payload of the current node.
            pub fn get(&self) -> &N {
                self.parent.get(self.at())
            }
        
            /// Makes the cursor point to the `dst`.
            pub fn jump(&mut self, dst : GraphPtr<'id, NodeType>) {
                self.current = dst;
            }
        }
        
        impl <'this, 'id : 'this, N : 'this, E : 'this>
        $cursor_type<'this, 'id, NamedNode<N, E>>
        {
            /// Returns an iterator over edges and node pointers attached to the current node.
            pub fn iter(&self) -> impl Iterator<Item = GraphIterRes<Edge<&'_ N, &'_ E>, GraphPtr<'id, NamedNode<N, E>>>> {
                let current = self.at().as_ptr();
                let g = self.at()._guard;
        
                let node_refs = unsafe { &(*current).refs };
                node_refs.iter().map(move |x| {
                    let ptr = *(x.0);
                    let p =  unsafe { GraphPtr::from_ptr(ptr, g) };
                    let that = unsafe { (*ptr).get() };

                    if (current == ptr) {
                        GraphIterRes { values : Loop(EdgeSingle { this : that, edge : x.1}), ptr : p }
                    } else {
                        let this = unsafe { (*current).get() };
                        GraphIterRes { values : Both(EdgeBoth { this : this, that : that, edge : x.1 }), ptr : p }
                    }
                })
            }
        
            /// Returns Some if `dst` is attached to the current node and None otherwise.
            pub fn get_edge(&self, dst : GraphPtr<'id, NamedNode<N, E>>) -> Option<Edge<&'_ N, &'_ E>> {
                self.parent.get_edge(self.at(), dst)
            }
        }
        
        impl <'this, 'id : 'this, N : 'this, E : 'this> Deref for $cursor_type<'this, 'id, NamedNode<N, E>> {
            type Target = node_views::NamedNode<'id, N, E>;
            fn deref(&self) -> &Self::Target {
                // not sound, but should be
                let this = unsafe { &(*self.at().as_ptr()) };
                unsafe { transmute(this) }
            }
        }
    }
}

impl_cursor_immutable!{CursorMut}
impl_cursor_immutable!{Cursor}

impl <'this, 'id : 'this, N : 'this, E : 'this, NodeType : 'this>
CursorMut<'this, 'id, NodeType>
where NodeType : GraphNode<Node = N, Edge = E>
{
    /// Returns mutable references to payloads of the current node and `dst` as if two are connected via an
    /// empty edge.
    pub fn bridge(&mut self, dst : GraphPtr<'id, NodeType>) -> Edge<&'_ mut N, ()> {
        self.parent.bridge(self.at(), dst)
    }

    /// Returns a mutable reference to payload of the current node.
    pub fn get_mut(&mut self) -> &mut N {
        self.parent.get_mut(self.at())
    }
}

impl <'this, 'id : 'this, N : 'this, E : 'this>
CursorMut<'this, 'id, NamedNode<N, E>>
{
    /// Returns a mutable iterator over edges and node pointers attached to the current node.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = GraphIterRes<Edge<&'_ mut N, &'_ mut E>, GraphPtr<'id, NamedNode<N, E>>>> {
        let current = self.at().as_mut();
        let g = self.at()._guard;

        //*current is dropped before closure is ever invoked and does not alias
        let node_refs = unsafe { &mut (*current).refs };
        node_refs.iter_mut().map(move |x| {
            let ptr = *(x.0) as *mut NamedNode<N, E>;
            let p =  unsafe { GraphPtr::from_mut(ptr, g) };
            let that = unsafe { (*ptr).get_mut() };

            if current == ptr {
                GraphIterRes { values : Loop(EdgeSingle { this : that, edge : x.1}), ptr : p }
            } else {
                //aliasing was explicitely checked
                let this = unsafe { (*current).get_mut() };
                GraphIterRes { values : Both(EdgeBoth { this , that, edge : x.1 }), ptr : p }
            }
        })
    }

    /// Creates an edge from the current node to `dst` with the value edge.
    pub fn attach(&mut self, dst : GraphPtr<'id, NamedNode<N, E>>, edge : E) {
        self.parent.connect(self.at(), dst, edge);
    }

    /// Removes an edge from the node to `dst`.
    pub fn detach(&mut self, dst : GraphPtr<'id, NamedNode<N, E>>) {
        self.parent.disconnect(self.at(), dst);
    }

    /// Returns Some if `dst` is attached to the current node and None otherwise.
    fn get_edge_mut(&mut self, dst : GraphPtr<'id, NamedNode<N, E>>) -> Option<Edge<&'_ mut N, &'_ mut E>> {
        self.parent.get_edge_mut(self.at(), dst)
    }
}

impl <'this, 'id : 'this, N : 'this, E : 'this> DerefMut for CursorMut<'this, 'id, NamedNode<N, E>> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        let this = unsafe { &mut (*self.at().as_mut()) };
        //unsound
        unsafe { transmute(this) }
    }
}
