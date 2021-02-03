#![allow(unused_unsafe)]

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
where Root : RootCollection<'static, NodeType>,
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
    fn cleanup(&mut self) {
        self.cleanup_precise();
    }
}

impl <Root, NodeType> Default for GenericGraph<Root, NodeType>
where Root : RootCollection<'static, NodeType>,
      NodeType : GraphNode
{
    fn default() -> Self
    {
        GenericGraph::new()
    }
}

impl <Root, NodeType> GenericGraph<Root, NodeType>
where Root : RootCollection<'static, NodeType>,
      NodeType : GraphNode
{
    pub fn new() -> Self
    {
        GenericGraph { root : Root::default(), internal : GraphRaw::new() }
    }
}

impl <Root, NodeType> GenericGraph<Root, NodeType>
where Root : RootCollection<'static, NodeType>,
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

pub type VecGraph<T> = GenericGraph<RootVec<'static, T>, T>;
pub type NamedGraph<T> = GenericGraph<RootNamedSet<'static, T>, T>;
pub type OptionGraph<T> = GenericGraph<RootOption<'static, T>, T>;

/// A strategy AnchorMut employs to perform cleanup after drop.
pub enum CleanupStrategy {
    /// AnchorMut never cleans up.
    Never,
    /// AnchorMut always performs cleanup when dropped
    Always,
    /// AnchorMut always performs precise cleanup when dropped
    AlwaysPrecise
}

pub struct AnchorMut<'this, 'id, T : 'this>
where T : GraphImpl
{
    parent: &'this mut T,
    strategy : CleanupStrategy,
    _guard : Id<'id>,
}

pub struct Anchor<'this, 'id, T : 'this>
where T : GraphImpl
{
    parent: &'this T,
    strategy : CleanupStrategy,
    _guard : Id<'id>,
}

impl <Root, NodeType> GraphImpl
for GenericGraph<Root, NodeType>
where Root : RootCollection<'static, NodeType>,
      NodeType : GraphNode
{
    fn cleanup_precise(&mut self) {
        self.internal.cleanup_precise(&self.root);
    }
}

impl <'this, 'id, T : 'this> Drop for AnchorMut<'this, 'id, T>
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

macro_rules! impl_anchor_index {
    ($NodeType:ident) => {
        impl <'this, 'id, N : 'this, E : 'this, Root : 'this>
        Index<GraphPtr<'id, $NodeType<N, E>>>
        for Anchor<'this, 'id, GenericGraph<Root, $NodeType<N, E>>>
        where Root : RootCollection<'static, $NodeType<N, E>>
        {
            type Output = node_views::$NodeType<'id, N, E>;
            fn index(&self, dst : GraphPtr<'id, $NodeType<N, E>>) -> &Self::Output
            {
                self.internal().get_view(dst)
            }
        }

        impl <'this, 'id, N : 'this, E : 'this, Root : 'this>
        Anchor<'this, 'id, GenericGraph<Root, $NodeType<N, E>>>
        where Root : RootCollection<'static, $NodeType<N, E>>
        {
            /// Returns an iterator over edges attached to `src` node.
            pub fn edges(&self, src : GraphPtr<'id, $NodeType<N, E>>) ->
                impl Iterator<Item = GraphItem<Edge<&'this N, &'this E>, GraphPtr<'id, $NodeType<N, E>>>>
            {
                self.internal().iter(src)
            }
        }
    }
}


impl_anchor_index!{NamedNode}
impl_anchor_index!{OptionNode}
impl_anchor_index!{VecNode}

impl <'this, 'id, K : 'this, N : 'this, E : 'this, Root : 'this>
Index<GraphPtr<'id, TreeNode<K, N, E>>>
for Anchor<'this, 'id, GenericGraph<Root, TreeNode<K, N, E>>>
where Root : RootCollection<'static, TreeNode<K, N, E>>, K : Ord
{
    type Output = node_views::TreeNode<'id, K, N, E>;
    fn index(&self, dst : GraphPtr<'id, TreeNode<K, N, E>>) -> &Self::Output
    {
        self.internal().get_view(dst)
    }
}

impl <'this, 'id, K : 'this, N : 'this, E : 'this, Root : 'this>
Anchor<'this, 'id, GenericGraph<Root, TreeNode<K, N, E>>>
where Root : RootCollection<'static, TreeNode<K, N, E>>, K : Ord
{
    /// Returns an iterator over edges attached to `src` node.
    pub fn edges(&self, src : GraphPtr<'id, TreeNode<K, N, E>>) ->
        impl Iterator<Item = GraphItem<Edge<&'this N, &'this E>, GraphPtr<'id, TreeNode<K, N, E>>>>
    {
        self.internal().iter(src)
    }
}


macro_rules! impl_anchor_mut_index {
    ($NodeType:ident) => {
        impl <'this, 'id, N : 'this, E : 'this, Root : 'this>
        Index<GraphPtr<'id, $NodeType<N, E>>>
        for AnchorMut<'this, 'id, GenericGraph<Root, $NodeType<N, E>>>
        where Root : RootCollection<'static, $NodeType<N, E>>
        {
            type Output = node_views::$NodeType<'id, N, E>;
            fn index(&self, dst : GraphPtr<'id, $NodeType<N, E>>) -> &Self::Output
            {
                self.internal().get_view(dst)
            }
        }

        impl <'this, 'id, N : 'this, E : 'this, Root : 'this>
        AnchorMut<'this, 'id, GenericGraph<Root, $NodeType<N, E>>>
        where Root : RootCollection<'static, $NodeType<N, E>>
        {
            /// Returns an iterator over edges attached to `src` node.
            pub fn edges(&self, src : GraphPtr<'id, $NodeType<N, E>>) ->
                impl Iterator<Item = GraphItem<Edge<&'_ N, &'_ E>, GraphPtr<'id, $NodeType<N, E>>>>
            {
                self.internal().iter(src)
            }
        }

        impl <'this, 'id, N : 'this, E : 'this, Root : 'this>
        IndexMut<GraphPtr<'id, $NodeType<N, E>>>
        for AnchorMut<'this, 'id, GenericGraph<Root, $NodeType<N, E>>>
        where Root : RootCollection<'static, $NodeType<N, E>>
        {
            fn index_mut(&mut self, dst : GraphPtr<'id, $NodeType<N, E>>) -> &mut Self::Output {
                self.internal_mut().get_view_mut(dst)
            }
        }
        
        impl <'this, 'id, N : 'this, E : 'this, Root : 'this>
        AnchorMut<'this, 'id, GenericGraph<Root, $NodeType<N, E>>>
        where Root : RootCollection<'static, $NodeType<N,E>>
        {
            /// Returns a mutable iterator over edges attached to `src` node.
            pub fn edges_mut(&mut self, src : GraphPtr<'id, $NodeType<N, E>>) ->
                impl Iterator<Item = GraphItem<Edge<&'_ mut N, &'_ mut E>, GraphPtr<'id, $NodeType<N, E>>>>
            {
                self.internal_mut().iter_mut(src)
            }
        
            /// Provides direct mutable direct access to two different nodes `src` and `dst`. Returns or None if `src` is the same as `dst`.
            pub fn bridge(&mut self, src : GraphPtr<'id, $NodeType<N, E>>,
                                     dst : GraphPtr<'id, $NodeType<N, E>>) ->
                Option<(&'_ mut node_views::$NodeType<'id, N, E>, &'_ mut node_views::$NodeType<'id, N, E>)>
            {
                self.internal_mut().bridge(src, dst)
            }
        }
    }
}

impl_anchor_mut_index!{NamedNode}
impl_anchor_mut_index!{OptionNode}
impl_anchor_mut_index!{VecNode}

impl <'this, 'id, K : 'this, N : 'this, E : 'this, Root : 'this>
Index<GraphPtr<'id, TreeNode<K, N, E>>>
for AnchorMut<'this, 'id, GenericGraph<Root, TreeNode<K, N, E>>>
where Root : RootCollection<'static, TreeNode<K, N, E>>, K : Ord
{
    type Output = node_views::TreeNode<'id, K, N, E>;
    fn index(&self, dst : GraphPtr<'id, TreeNode<K, N, E>>) -> &Self::Output
    {
        self.internal().get_view(dst)
    }
}

impl <'this, 'id, K : 'this, N : 'this, E : 'this, Root : 'this>
AnchorMut<'this, 'id, GenericGraph<Root, TreeNode<K, N, E>>>
where Root : RootCollection<'static, TreeNode<K, N, E>>, K : Ord
{
    /// Returns an iterator over edges attached to `src` node.
    pub fn edges(&self, src : GraphPtr<'id, TreeNode<K, N, E>>) ->
        impl Iterator<Item = GraphItem<Edge<&'_ N, &'_ E>, GraphPtr<'id, TreeNode<K, N, E>>>>
    {
        self.internal().iter(src)
    }
}

impl <'this, 'id, K : 'this, N : 'this, E : 'this, Root : 'this>
IndexMut<GraphPtr<'id, TreeNode<K, N, E>>>
for AnchorMut<'this, 'id, GenericGraph<Root, TreeNode<K, N, E>>>
where Root : RootCollection<'static, TreeNode<K, N, E>>, K : Ord
{
    fn index_mut(&mut self, dst : GraphPtr<'id,  TreeNode<K, N, E>>) -> &mut Self::Output {
        self.internal_mut().get_view_mut(dst)
    }
}

impl <'this, 'id, K : 'this, N : 'this, E : 'this, Root : 'this>
AnchorMut<'this, 'id, GenericGraph<Root, TreeNode<K, N, E>>>
where Root : RootCollection<'static, TreeNode<K, N, E>>, K : Ord
{
    /// Returns a mutable iterator over edges attached to `src` node.
    pub fn edges_mut(&mut self, src : GraphPtr<'id, TreeNode<K, N, E>>) ->
        impl Iterator<Item = GraphItem<Edge<&'_ mut N, &'_ mut E>, GraphPtr<'id, TreeNode<K, N, E>>>>
    {
        self.internal_mut().iter_mut(src)
    }

    /// Provides direct mutable direct access to two different nodes `src` and `dst`. Returns or None if `src` is the same as `dst`.
    pub fn bridge(&mut self, src : GraphPtr<'id, TreeNode<K, N, E>>,
                             dst : GraphPtr<'id, TreeNode<K, N, E>>) ->
        Option<(&'_ mut node_views::TreeNode<'id, K, N, E>, &'_ mut node_views::TreeNode<'id, K, N, E>)>
    {
        self.internal_mut().bridge(src, dst)
    }
}

impl <'this, 'id, N : 'this, NodeType : 'this, Root : 'this>
AnchorMut<'this, 'id, GenericGraph<Root, NodeType>>
where NodeType : GraphNode<Node = N>,
      Root : RootCollection<'static, NodeType>
{
    fn internal(&self) -> &GraphRaw<NodeType> {
        &self.parent.internal
    }

    /// Creates a checked pointer from a raw pointer.
    /// # Safety
    /// Caller must guarantee `raw` points to a node which was not cleaned up and belongs to the parent graph. 
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

impl <'this, 'id, N : 'this, NodeType : 'this, Root : 'this>
Anchor<'this, 'id, GenericGraph<Root, NodeType>>
where NodeType : GraphNode<Node = N>,
      Root : RootCollection<'static, NodeType>
{
    fn internal(&self) -> &GraphRaw<NodeType> {
        &self.parent.internal
    }

    /// Creates a checked pointer from a raw pointer.
    /// # Safety
    /// Caller must guarantee `raw` points to a node which was not cleaned up and belongs to the parent graph. 
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

impl <'this, 'id, N : 'this, NodeType : 'this, Root : 'this>
AnchorMut<'this, 'id, GenericGraph<Root, NodeType>>
where NodeType : GraphNode<Node = N>,
      Root : RootCollection<'static, NodeType>
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

macro_rules! impl_root_mut_iter {
    ($root_type:ident) => {
        impl <'this, 'id, N : 'this, NodeType : 'this>
        AnchorMut<'this, 'id, $root_type<NodeType>>
        where NodeType : GraphNode<Node = N>
        {
            /// Returns an iterator over data and pointers to nodes attached to the root.
            pub fn iter(&self) -> impl Iterator<Item = GraphItem<&'_ N, GraphPtr<'id, NodeType>>>
            {
                self.root().iter().map(move |x| {
                    let p = x.as_ptr();
                    let values = unsafe { (*p).get() };
                    GraphItem { values, ptr : *x }
                })
            }

            /// Returns a mutable iterator over data and pointers to nodes attached to the root.
            pub fn iter_mut(&mut self) -> impl Iterator<Item = GraphItem<&'_ mut N, GraphPtr<'id, NodeType>>>
            {
                self.root_mut().iter().map(move |x| {
                    let p = x.as_mut();
                    let values = unsafe { (*p).get_mut() };
                    GraphItem { values, ptr : *x }
                })
            }
        }
    }
}

impl_root_mut_iter!{VecGraph}
impl_root_mut_iter!{NamedGraph}
impl_root_mut_iter!{OptionGraph}

/// A wrapper over a GraphPtr which provides simplified access to AnchorMut API.
pub struct CursorMut<'this, 'id, T : 'this> {
    parent : &'this mut GraphRaw<T>,
    current : GraphPtr<'id, T>
}

/// A wrapper over a GraphPtr which provides simplified access to Anchor API.
pub struct Cursor<'this, 'id, T : 'this> {
    parent : &'this GraphRaw<T>,
    current : GraphPtr<'id, T>
}

macro_rules! impl_cursor_immutable {
    ($cursor_type:ident) => {
        impl <'this, 'id, N : 'this, NodeType : 'this>
        $cursor_type<'this, 'id, NodeType>
        where NodeType : GraphNode<Node = N>
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
        
        impl <'this, 'id, N : 'this, E : 'this>
        $cursor_type<'this, 'id, NamedNode<N, E>>
        {    
            /// Returns Some if `dst` is attached to the current node and None otherwise.
            pub fn get_edge(&self, dst : GraphPtr<'id, NamedNode<N, E>>) -> Option<Edge<&'_ N, &'_ E>>
            {
                self.parent.get_edge(self.at(), dst)
            }
        }

        impl <'this, 'id, N : 'this, E : 'this>
        $cursor_type<'this, 'id, VecNode<N, E>>
        {    
            /// Returns Some if `dst` is attached to the current node and None otherwise.
            pub fn get_edge(&self, dst : usize) -> Option<Edge<&'_ N, &'_ E>>
            {
                self.parent.get_edge(self.at(), dst)
            }
        }

        impl <'this, 'id, N : 'this, E : 'this>
        $cursor_type<'this, 'id, OptionNode<N, E>>
        {    
            /// Returns Some if a node is attached to the current node and None otherwise.
            pub fn get_edge(&self, _dst : ()) -> Option<Edge<&'_ N, &'_ E>>
            {
                self.parent.get_edge(self.at())
            }
        }

        impl <'this, 'id, K : 'this, N : 'this, E : 'this>
        $cursor_type<'this, 'id, TreeNode<K, N, E>> where K : Ord
        {    
            /// Returns Some if a node is attached to the current node and None otherwise.
            pub fn get_edge(&self, dst : &K) -> Option<Edge<&'_ N, &'_ E>>
            {
                self.parent.get_edge(self.at(), dst)
            }
        }


        impl <'this, 'id, K : 'this, N : 'this, E : 'this>
        $cursor_type<'this, 'id, TreeNode<K, N, E>> where K : Ord
        {
            /// Returns an iterator over edges and node pointers attached to the current node.
            pub fn edges(&self) ->
                impl Iterator<Item = GraphItem<Edge<&'_ N, &'_ E>, GraphPtr<'id, TreeNode<K, N, E>>>>
            {
                self.parent.iter(self.at())
            }
        }
        
        impl <'this, 'id, K : 'this, N : 'this, E : 'this> Deref for $cursor_type<'this, 'id, TreeNode<K, N, E>> where K : Ord
        {
            type Target = node_views::TreeNode<'id, K, N, E>;
            fn deref(&self) -> &Self::Target
            {
                self.parent.get_view(self.at())
            }
        }


    };
    ($cursor_type:ident, $node_type:ident) => {
        impl <'this, 'id, N : 'this, E : 'this>
        $cursor_type<'this, 'id, $node_type<N, E>>
        {
            /// Returns an iterator over edges and node pointers attached to the current node.
            pub fn edges(&self) ->
                impl Iterator<Item = GraphItem<Edge<&'_ N, &'_ E>, GraphPtr<'id, $node_type<N, E>>>>
            {
                self.parent.iter(self.at())
            }
        }
        
        impl <'this, 'id, N : 'this, E : 'this> Deref for $cursor_type<'this, 'id, $node_type<N, E>>
        {
            type Target = node_views::$node_type<'id, N, E>;
            fn deref(&self) -> &Self::Target
            {
                self.parent.get_view(self.at())
            }
        }
    };
}

impl_cursor_immutable!{CursorMut}
impl_cursor_immutable!{Cursor}

impl_cursor_immutable!{CursorMut, NamedNode}
impl_cursor_immutable!{Cursor, NamedNode}
impl_cursor_immutable!{CursorMut, VecNode}
impl_cursor_immutable!{Cursor, VecNode}
impl_cursor_immutable!{CursorMut, OptionNode}
impl_cursor_immutable!{Cursor, OptionNode}

impl <'this, 'id, N : 'this, E : 'this>
CursorMut<'this, 'id, NamedNode<N, E>>
{    
    /// Returns Some if `dst` is attached to the current node and None otherwise.
    pub fn get_edge_mut(&mut self, dst : GraphPtr<'id, NamedNode<N, E>>) -> Option<Edge<&'_ mut N, &'_ mut E>>
    {
        self.parent.get_edge_mut(self.at(), dst)
    }
}

impl <'this, 'id, N : 'this, E : 'this>
CursorMut<'this, 'id, VecNode<N, E>>
{    
    /// Returns Some if `dst` is attached to the current node and None otherwise.
    pub fn get_edge_mut(&mut self, dst : usize) -> Option<Edge<&'_ mut N, &'_ mut E>>
    {
        self.parent.get_edge_mut(self.at(), dst)
    }
}

impl <'this, 'id, N : 'this, E : 'this>
CursorMut<'this, 'id, OptionNode<N, E>>
{    
    /// Returns Some if a node is attached to the current node and None otherwise.
    pub fn get_edge_mut(&mut self, _key : ()) -> Option<Edge<&'_ mut N, &'_ mut E>>
    {
        self.parent.get_edge_mut(self.at())
    }
}

macro_rules! impl_cursor_mut {
    ($node_type:ident) => {
        impl <'this, 'id, N : 'this, E : 'this>
        CursorMut<'this, 'id, $node_type<N, E>>
        {
            /// Returns a mutable iterator over edges and node pointers attached to the current node.
            pub fn edges_mut(&mut self) ->
                impl Iterator<Item = GraphItem<Edge<&'_ mut  N, &'_ mut E>, GraphPtr<'id, $node_type<N, E>>>>
            {
                self.parent.iter_mut(self.at())
            }

            /// Provides direct mutable access to current and `dst` nodes or or None if current is the same as `dst`.
            /// Returns mutable views into the current and `dst` nodes or None if current is the same as `dst`.
            pub fn bridge(&mut self, dst : GraphPtr<'id, $node_type<N, E>>) ->
                Option<(&'_ mut node_views::$node_type<'id, N, E>, &'_ mut node_views::$node_type<'id, N, E>)>
            {
                self.parent.bridge(self.at(), dst)
            }
        }

        impl <'this, 'id, N : 'this, E : 'this> DerefMut for CursorMut<'this, 'id, $node_type<N, E>>
        {
            fn deref_mut(&mut self) -> &mut Self::Target {
                self.parent.get_view_mut(self.at())
            }
        }
    }
}

impl_cursor_mut!{NamedNode}
impl_cursor_mut!{VecNode}
impl_cursor_mut!{OptionNode}

impl <'this, 'id, K : 'this, N : 'this, E : 'this>
CursorMut<'this, 'id, TreeNode<K, N, E>> where K : Ord
{
    /// Returns a mutable iterator over edges and node pointers attached to the current node.
    pub fn edges_mut(&mut self) ->
        impl Iterator<Item = GraphItem<Edge<&'_ mut  N, &'_ mut E>, GraphPtr<'id, TreeNode<K, N, E>>>>
    {
        self.parent.iter_mut(self.at())
    }

    /// Provides direct mutable access to current and `dst` nodes or or None if current is the same as `dst`.
    /// Returns mutable views into the current and `dst` nodes or None if current is the same as `dst`.
    pub fn bridge(&mut self, dst : GraphPtr<'id, TreeNode<K, N, E>>) ->
        Option<(&'_ mut node_views::TreeNode<'id, K, N, E>, &'_ mut node_views::TreeNode<'id, K, N, E>)>
    {
        self.parent.bridge(self.at(), dst)
    }
}

impl <'this, 'id, K : 'this, N : 'this, E : 'this>
DerefMut for CursorMut<'this, 'id, TreeNode<K, N, E>> where K : Ord
{
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.parent.get_view_mut(self.at())
    }
}

macro_rules! impl_generic_graph_root {
    ($collection:ident, $graph:ident) => {
        impl <'this, 'id, N : 'this, NodeType : 'this>
        AnchorMut<'this, 'id, $graph<NodeType>>
        where NodeType : GraphNode<Node = N>
        {
            /// Provides direct access to the collection of the root.
            pub fn root(&self) -> &$collection<'id, NodeType>
            {
                //this transmute only affects lifetime parameter
                unsafe {
                    transmute(&self.parent.root)
                }
            }

            /// Provides direct mutable access to the collection of the root.
            pub fn root_mut(&mut self) -> &mut $collection<'id, NodeType>
            {
                //this transmute only affects lifetime parameter
                unsafe {
                    transmute(&mut self.parent.root)
                }
            }
        }
    }
}

impl_generic_graph_root!{RootVec, VecGraph}
impl_generic_graph_root!{RootNamedSet, NamedGraph}
impl_generic_graph_root!{RootOption, OptionGraph}

#[macro_export]
/// Creates an AnchorMut using selected cleanup strategy.
macro_rules! anchor_mut
{
    ($name:ident, $strategy:tt) => {
        make_guard!(g);
        let mut $name = unsafe { $name.anchor_mut(Id::from(g), $strategy)   };
    };
    ($name:ident, $parent:tt, $strategy:tt) => {
        make_guard!(g);
        let mut $name = unsafe { $parent.anchor_mut(Id::from(g), $strategy) };
    };
}