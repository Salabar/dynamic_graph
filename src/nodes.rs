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

    //TODO: use SmallBox
    //TODO2: use impl Iterator when available or monomorphise manually
    fn edge_ptrs<'a>(&'a self) -> Box<dyn Iterator<Item = *mut Self> + 'a>;
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

    fn edge_ptrs<'a>(&'a self) -> Box<dyn Iterator<Item = *mut Self> + 'a> {
        Box::new(self.internal.refs.iter().map(|x| { x.0.as_mut() }))
    }

    fn from_data(data : Self::Node) -> Self
    {
        let meta = MetaData { cleanup_gen : CleanupGen::Even, store_index : 0 };
        Self { internal : node_views::NamedNode { refs : HashMap::new(), data }, meta }
    }
}