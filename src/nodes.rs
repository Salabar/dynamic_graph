use super::*;

use std::collections::{HashMap, HashSet, BTreeMap};

#[derive(PartialEq, Eq, Clone, Copy)]
pub enum CleanupGen {
    Even, Odd
}

#[derive(PartialEq, Eq, Clone, Copy)]
pub struct MetaData {
    pub(crate) cleanup_gen : CleanupGen,
    pub(crate) store_index: usize,
}

impl CleanupGen {
    pub(crate) fn flip(&mut self) {
        if *self == CleanupGen::Even {
            *self = CleanupGen::Odd
        } else {
            *self = CleanupGen::Even
        }
    }
}

pub trait GraphNode : Sized {
    type Node;
    fn get(&self) -> &Self::Node;
    fn get_mut(&mut self) -> &mut Self::Node;

    fn meta(&self) -> &MetaData;
    fn meta_mut(&mut self) -> &mut MetaData;

    fn traverse(&self, cleanup : &mut CleanupState<Self>);

    fn from_data(data : Self::Node) -> Self;
}
/// A node type which uses node pointers as keys in the edge collection.
pub struct NamedNode<N, E> {
    pub(crate) internal: node_views::NamedNode<'static, N, E>,
    pub(crate) meta : MetaData,
}

/// Views into nodes allowing direct access to the nodes data and references.
pub mod node_views {
    use super::*;
    pub struct NamedNode<'id, N, E> {
        pub refs : NodeNamedMap<'id, super::NamedNode<N, E>, E>,
        pub data : N,
    }
}

impl <N, E> NamedNode<N, E> {
    pub (crate) fn get_view<'id>(&self) -> &node_views::NamedNode<'id, N, E> {
        unsafe {
            transmute(&self.internal)
        }
    }

    pub (crate) fn get_view_mut<'id>(&mut self) -> &mut node_views::NamedNode<'id, N, E> {
        unsafe {
            transmute(&mut self.internal)
        }
    }
}

impl <N, E> GraphNode for NamedNode<N, E> {
    type Node = N;

    fn get(&self) -> &Self::Node
    {
        &self.internal.data
    }

    fn get_mut(&mut self) -> &mut Self::Node
    {
        &mut self.internal.data
    }

    fn meta(&self) -> &MetaData {
        &self.meta
    }
    
    fn meta_mut(&mut self) -> &mut MetaData {
        &mut self.meta
    }

    fn traverse(&self, cleanup : &mut CleanupState<Self>) {
        NodeCollection::traverse(&self.internal.refs, cleanup);
    }

    fn from_data(data : Self::Node) -> Self
    {
        let meta = MetaData { cleanup_gen : CleanupGen::Even, store_index : 0 };
        Self { internal : node_views::NamedNode { refs : HashMap::new(), data }, meta }
    }
}

pub unsafe trait NodeCollection : Default {
    type NodeType : GraphNode;
    fn traverse(this : &Self, cleanup : &mut CleanupState<Self::NodeType>);
}

pub unsafe trait RootCollection : Default {
    type NodeType : GraphNode;
    fn traverse(this : &Self, cleanup : &mut CleanupState<Self::NodeType>);
}

fn traverse_touch<NodeType : GraphNode>(iter : impl Iterator<Item = *mut NodeType>, cleanup : &mut CleanupState<NodeType>) {
    for i in iter {
        cleanup.touch(i);
    }
}

pub type RootVec<'id, T> = Vec<GraphPtr<'id, T>>;
pub type RootNamedSet<'id, T> = HashSet<GraphPtr<'id, T>>;
pub type RootHashMap<'id, K, T> = HashMap<K, GraphPtr<'id, T>>;
pub type RootOption<'id, T> = Option<GraphPtr<'id, T>>;

pub type NodeVec<'id, NodeType, E> = Vec<(GraphPtr<'id, NodeType>, E)>;
pub type NodeNamedMap<'id, NodeType, E> = HashMap<GraphPtr<'id, NodeType>, E>;
pub type NodeTreeMap<'id, K, NodeType, E> = BTreeMap<K, (GraphPtr<'id, NodeType>, E)>;
pub type NodeOption<'id, NodeType, E> = Option<(GraphPtr<'id, NodeType>, E)>;

macro_rules! impl_root_collection {
    ($collection:ident) => {
        unsafe impl <'id, NodeType> RootCollection for $collection<'id, NodeType>
        where NodeType : GraphNode
        {
            type NodeType = NodeType;
            fn traverse(this : &Self, cleanup : &mut CleanupState<NodeType>) {
                traverse_touch(this.iter().map(|x| x.as_mut()), cleanup);
            }
        }
    }
}

unsafe impl <'id, K, NodeType> RootCollection for RootHashMap<'id, K, NodeType>
where NodeType : GraphNode,
      K : Hash + Eq
{
    type NodeType = NodeType;
    fn traverse(this : &Self, cleanup : &mut CleanupState<NodeType>) {
        traverse_touch(this.values().map(|x| x.as_mut()), cleanup);
    }
}

impl_root_collection!{RootVec}
impl_root_collection!{RootNamedSet}
impl_root_collection!{RootOption}

macro_rules! impl_node_collection {
    ($collection:ident) => {
        unsafe impl <'id, NodeType, E> NodeCollection for $collection<'id, NodeType, E>
        where NodeType : GraphNode
        {
            type NodeType = NodeType;
            fn traverse(this : &Self, cleanup : &mut CleanupState<NodeType>) {
                traverse_touch(this.iter().map(|x| x.0.as_mut()), cleanup);
            }
        }
    }
}

impl_node_collection!{NodeVec}
impl_node_collection!{NodeNamedMap}
impl_node_collection!{NodeOption}

unsafe impl <'id, K, NodeType, E> NodeCollection for NodeTreeMap<'id, K, NodeType, E>
where NodeType : GraphNode,
      K : Ord
{
    type NodeType = NodeType;
    fn traverse(this : &Self, cleanup : &mut CleanupState<NodeType>) {
        traverse_touch(this.values().map(|x| x.0.as_mut()), cleanup);
    }
}
