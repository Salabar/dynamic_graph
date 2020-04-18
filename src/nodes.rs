use super::*;

use std::collections::HashMap;

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


/// A node type which uses node pointers as keys in the edge collection.
pub struct NamedNode<N, E> {
    pub(crate) internal: node_views::NamedNode<'static, N, E>,
    pub(crate) meta : MetaData,
}

/// Views into nodes allowing direct access to the nodes data and references.
pub mod node_views {
    use super::*;

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

    fn meta(&self) -> &MetaData;
    fn meta_mut(&mut self) -> &mut MetaData;

    fn traverse(&self, cleanup : &mut CleanupState<Self>);

    fn from_data(data : Self::Node) -> Self;
}

impl <N, E> GraphNode for NamedNode<N, E> {
    type Node = N;
    type Edge = E;

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

pub trait NodeCollection : Sized {
    type NodeType;
    fn traverse(this : &Self, cleanup : &mut CleanupState<Self::NodeType>);
}

impl <NodeType> NodeCollection for Vec<GraphPtr<'static, NodeType>>
where NodeType : GraphNode
{
    type NodeType = NodeType;
    fn traverse(this : &Self, cleanup : &mut CleanupState<NodeType>) {
        for i in this.iter().map(|x| x.as_mut()) {
            cleanup.touch(i);
        }
    }
}


impl <N, E, NodeType> NodeCollection for HashMap<GraphPtr<'static, NodeType>, E>
where NodeType : GraphNode<Node = N>
{
    type NodeType = NodeType;
    fn traverse(this : &Self, cleanup : &mut CleanupState<NodeType>) {
        for i in this.iter().map(|x| x.0.as_mut()) {
            cleanup.touch(i);
        }
    }
}