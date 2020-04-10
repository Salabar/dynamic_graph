pub mod graph_ptr;
pub use crate::graph_ptr::*;

mod graph_raw;
use crate::graph_raw::*;

pub mod edge;
pub use crate::edge::*;

pub mod nodes;
pub use crate::nodes::*;

use core::hash::{Hash, Hasher};
use core::mem::transmute;
use core::ops::{Index, IndexMut, Deref, DerefMut};
use core::ptr::NonNull;

pub struct GenericGraph<Root, NodeType>
where Root : RootCollection,
      NodeType : GraphNode,
{
    internal : GraphRaw<NodeType>,
    root : Root
}

pub trait GraphImpl {
    /// Traverses the graph and drops any inaccessible node. Disregards any heuristic designed to improve
    /// cleanup performance.
    fn cleanup_precise(&mut self);
    /// Traverses the graph and drops inaccessible nodes. This method will miss some of the leaked items which
    /// might result in spikes in memory usage. !! Currently, none of the possible heuristics are implemented.
    fn cleanup(&mut self);
}

impl <Root, NodeType> Default for GenericGraph<Root, NodeType>
where Root : RootCollection + Default,
      NodeType : GraphNode
{
    fn default() -> Self
    {
        GenericGraph::new()
    }
}

impl <Root, NodeType> GenericGraph<Root, NodeType>
where Root : RootCollection + Default,
      NodeType : GraphNode
{
    pub fn new() -> Self
    {
        GenericGraph { root : Root::default(), internal : GraphRaw::new() }
    }
}

impl <Root, NodeType> GenericGraph<Root, NodeType>
where Root : RootCollection<NodeType = NodeType> + Default,
      NodeType : GraphNode
{
    /// Creates an AnchorMut from a generativity brand using selected cleanup strategy.
    /// Prefer `anchor_mut!` macro in application code.
    /// # Safety
    /// Caller must use a unique `guard` from generativity::Guard.
    pub unsafe fn anchor_mut<'id>(&mut self, guard : Id<'id>, strategy : CleanupStrategy)
                                  -> AnchorMut<'_, 'id, GenericGraph<Root, NodeType>>
    {
        AnchorMut { parent : self, _guard : guard, strategy }
    }
}

pub type VecGraph<T> = GenericGraph<Vec<GraphPtr<'static, T>>, T>;

pub trait RootCollection {
    type NodeType;
    //TODO: use SmallBox crate
    //TODO2: use impl Iterator when available or monomophise manually
    //LLVM SHOULD be able to devirtualize this iterator so performance hit SHOULDN'T be critical.
    fn root_ptrs<'a>(&'a self) -> Box<dyn Iterator<Item = *mut Self::NodeType> + 'a>;
}

impl <NodeType> RootCollection for Vec<GraphPtr<'static, NodeType>>
where NodeType : GraphNode
{
    type NodeType = NodeType;
    fn root_ptrs<'a>(&'a self) -> Box<dyn Iterator<Item = *mut Self::NodeType> + 'a>
    {
        Box::new(self.iter().map(|x| { x.as_mut() }))
    }
}

/// A strategy AnchorMut employs to perform cleanup after drop.
pub enum CleanupStrategy {
    /// AnchorMut never cleans up.
    Never,
    /// AnchorMut always performs cleanup when dropped
    Always,
    /// AnchorMut always performs precise cleanup when dropped
    AlwaysPrecise
}

pub struct AnchorMut<'this, 'id : 'this, T : 'this>
where T : GraphImpl
{
    parent: &'this mut T,
    strategy : CleanupStrategy,
    _guard : Id<'id>,
}

impl <Root, NodeType> GraphImpl
for GenericGraph<Root, NodeType>
where Root : RootCollection<NodeType = NodeType>,
      NodeType : GraphNode
{
    fn cleanup_precise(&mut self) {
        let iter = self.root.root_ptrs();
        self.internal.cleanup_precise(iter);
    }

    fn cleanup(&mut self) {
        self.cleanup_precise();
    }
}


impl <'this, 'id: 'this, T : 'this> Drop for AnchorMut<'this, 'id, T>
where T : GraphImpl
{
    fn drop(&mut self) {
        match &self.strategy {
            CleanupStrategy::AlwaysPrecise => self.parent.cleanup_precise(),
            CleanupStrategy::Always => self.parent.cleanup(),
            _ => ()
        }
    }
}

impl <'this, 'id : 'this, N : 'this, E : 'this, Root : 'this>
Index<GraphPtr<'id, NamedNode<N, E>>>
for AnchorMut<'this, 'id, GenericGraph<Root, NamedNode<N, E>>>
where Root : RootCollection<NodeType = NamedNode<N, E>>
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
where Root : RootCollection<NodeType = NamedNode<N, E>>
{
    fn index_mut(&mut self, dst : GraphPtr<'id, NamedNode<N, E>>) -> &mut Self::Output {
        self.internal_mut().get_view_mut(dst)
    }
}

#[macro_export]
/// Creates an AnchorMut using selected cleanup strategy.
macro_rules! anchor_mut
{
    ($name:ident, $strategy:tt) => {
        make_guard!(g);
        let mut $name = unsafe { $name.anchor_mut(Id::from(g), $strategy) };
    };
}

impl <'this, 'id : 'this, N : 'this, E : 'this, NodeType : 'this, Root : 'this>
AnchorMut<'this, 'id, GenericGraph<Root, NodeType>>
where NodeType : GraphNode<Node = N, Edge = E>,
      Root : RootCollection<NodeType = NodeType>
{
    fn internal(&self) -> &GraphRaw<NodeType> {
        &self.parent.internal
    }

    /// Creates a checked pointer from a raw pointer.
    /// # Safety
    /// Caller must guarantee `raw` points to a node which was not cleaned up and belongs to the parent graph. 
    /// If you really have to use a node from another graph, do not attach this pointer to the current collection.
    pub unsafe fn from_raw(&self, raw : *const NodeType) -> GraphPtr<'id, NodeType>
    {
        GraphPtr::from_ptr(raw, self._guard)
    }

    /// Creates an immutable cursor pointing to `dst`
    pub fn cursor(&self, dst : GraphPtr<'id, NodeType>) -> Cursor<'_, 'id, NodeType>
    {
        Cursor { parent : self.internal(), current : dst }
    }
}

impl <'this, 'id : 'this, N : 'this, E : 'this, NodeType : 'this, Root : 'this>
AnchorMut<'this, 'id, GenericGraph<Root, NodeType>>
where NodeType : GraphNode<Node = N, Edge = E>,
      Root : RootCollection<NodeType = NodeType>
{
    fn internal_mut(&mut self) -> &mut GraphRaw<NodeType>
    {
        &mut self.parent.internal
    }

    /// Allocates a new node and returns the pointer. This node will become inaccessible when parent anchor
    /// is dropped and will be disposed of upon next cleanup unless you attach it to the root or another node accessible
    /// from the root.
    pub fn spawn(&mut self, data : N) -> GraphPtr<'id, NodeType>
    {
        let ptr = self.internal_mut().spawn_detached(data);
        unsafe {
            //allocation never fails
            GraphPtr::from_ptr(ptr, self._guard )
        }
    }

    /// Immediately drops `dst` node and frees allocated memory.
    /// # Safety
    /// Caller must ensure killed node will never be accessed. `dst` must become inaccesible from root before
    /// anchor is dropped. Any copies of `dst` in external collections should be disposed of as well.
    pub unsafe fn kill(&mut self, dst : GraphPtr<'id, NodeType>) {
        self.internal_mut().kill(dst.as_mut());
    }

    /// Creates a mutable cursor pointing to `dst`.
    pub fn cursor_mut(&mut self, dst : GraphPtr<'id, NodeType>)
           -> CursorMut<'_, 'id, NodeType>
    {
        CursorMut { parent : self.internal_mut(), current : dst }
    }
}


impl <'this, 'id : 'this, N : 'this, E : 'this, NodeType : 'this>
AnchorMut<'this, 'id, VecGraph<NodeType>>
where NodeType : GraphNode<Node = N, Edge = E>
{
    /// Allocates a new node and returns the pointer. Attaches the node to the root by Vec::push.
    pub fn spawn_attached(&mut self, data : N) -> GraphPtr<'id, NodeType>
    {
        let res = self.spawn(data);
        let a = res.into_static();
        self.parent.root.push(a);
        res
    }

    /// Provides direct access to the collection of the root.
    pub fn root(&self) -> &Vec<GraphPtr<'id, NodeType>>
    {
        //this transmute only affects lifetime parameter
        unsafe {
            transmute(&self.parent.root)
        }
    }

    /// Provides direct mutable access to the collection of the root.
    pub fn root_mut(&mut self) -> &mut Vec<GraphPtr<'id, NodeType>>
    {
        //this transmute only affects lifetime parameter
        unsafe {
            transmute(&mut self.parent.root)
        }
    }
}

impl <'this, 'id : 'this, N : 'this, E : 'this, Root : 'this>
AnchorMut<'this, 'id, GenericGraph<Root, NamedNode<N, E>>>
where Root : RootCollection<NodeType = NamedNode<N, E>>
{
    /// Returns an iterator over edges attached to `src` node.
    pub fn edges(&self, src : GraphPtr<'id, NamedNode<N, E>>) ->
        impl Iterator<Item = GraphIterRes<Edge<&'_ N, &'_ E>, GraphPtr<'id, NamedNode<N, E>>>>
    {
        self.internal().iter(src)
    }
}

impl <'this, 'id : 'this, N : 'this, E : 'this, Root : 'this>
AnchorMut<'this, 'id, GenericGraph<Root, NamedNode<N, E>>>
where Root : RootCollection<NodeType = NamedNode<N,E>>
{
    /// Returns a mutable iterator over edges attached to `src` node.
    pub fn edges_mut(&mut self, src : GraphPtr<'id, NamedNode<N, E>>) ->
        impl Iterator<Item = GraphIterRes<Edge<&'_ mut N, &'_ mut E>, GraphPtr<'id, NamedNode<N, E>>>>
    {
        self.internal_mut().iter_mut(src)
    }

    /// Provides direct mutable direct access to two different nodes `src` and `dst`. Returns or None if `src` is the same as `dst`.
    pub fn bridge(&mut self, src : GraphPtr<'id, NamedNode<N, E>>,
                             dst : GraphPtr<'id, NamedNode<N, E>>) ->
        Option<(&'_ mut node_views::NamedNode<'id, N, E>, &'_ mut node_views::NamedNode<'id, N, E>)>
    {
        self.internal_mut().bridge(src, dst)
    }

}

impl <'this, 'id : 'this, N : 'this, E : 'this>
AnchorMut<'this, 'id, VecGraph<NamedNode<N, E>>>
{
    /// Returns an iterator over views into nodes attached to the root.
    pub fn iter(&self) -> impl Iterator<Item = &'_ node_views::NamedNode<'id, N, E>>
    {
        self.root().iter().map(move |x| {
            &self[*x]
        })
    }

    /// Returns an iterator over views into nodes attached to the root.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &'_ mut node_views::NamedNode<'id, N, E>>
    {
        //GraphRaw.rs get_view_mut
        self.root_mut().iter_mut().map(move |x| {
            unsafe {
                transmute(&mut (*x.as_mut()).internal)
            }
        })
    }
}

/// A wrapper over a GraphPtr which provides simplified access to AnchorMut API.
pub struct CursorMut<'this, 'id : 'this, T : 'this> {
    parent : &'this mut GraphRaw<T>,
    current : GraphPtr<'id, T>
}

/// A wrapper over a GraphPtr which provides simplified access to Anchor API.
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
        
            /// Moves the cursor to `dst`.
            pub fn jump(&mut self, dst : GraphPtr<'id, NodeType>)
            {
                self.current = dst;
            }
        }
        
        impl <'this, 'id : 'this, N : 'this, E : 'this>
        $cursor_type<'this, 'id, NamedNode<N, E>>
        {
            /// Returns an iterator over edges and node pointers attached to the current node.
            pub fn edges(&self) ->
                impl Iterator<Item = GraphIterRes<Edge<&'_ N, &'_ E>, GraphPtr<'id, NamedNode<N, E>>>>
            {
                self.parent.iter(self.at())
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
    pub fn edges_mut(&mut self) ->
        impl Iterator<Item = GraphIterRes<Edge<&'_ mut  N, &'_ mut E>, GraphPtr<'id, NamedNode<N, E>>>>
    {
        self.parent.iter_mut(self.at())
    }

    /// Returns Some if `dst` is attached to the current node and None otherwise.
    fn get_edge_mut(&mut self, dst : GraphPtr<'id, NamedNode<N, E>>) -> Option<Edge<&'_ mut N, &'_ mut E>>
    {
        self.parent.get_edge_mut(self.at(), dst)
    }

    /// Provides direct mutable access to current and `dst` nodes or or None if current is the same as `dst`.
    /// Returns mutable views into the current and `dst` nodes or None if current is the same as `dst`.
    pub fn bridge(&mut self, dst : GraphPtr<'id, NamedNode<N, E>>) ->
        Option<(&'_ mut node_views::NamedNode<'id, N, E>, &'_ mut node_views::NamedNode<'id, N, E>)>
    {
        self.parent.bridge(self.at(), dst)
    }
}

impl <'this, 'id : 'this, N : 'this, E : 'this> DerefMut for CursorMut<'this, 'id, NamedNode<N, E>>
{
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.parent.get_view_mut(self.at())
    }
}