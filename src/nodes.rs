use super::*;

use std::collections::HashMap;
use core::mem::transmute;

#[derive(PartialEq, Eq, Clone, Copy)]
pub enum CleanupGen {
    Even, Odd
}

#[derive(PartialEq, Eq, Clone, Copy)]
pub struct ServiceData {
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
#[repr(C)]
pub struct NamedNode<N, E> {
    pub(crate) refs : HashMap<GraphPtr<'static, NamedNode<N, E>>, E>,
    pub(crate) data : N,
    pub(crate) service : ServiceData,
}

/// Views into nodes allowing direct access to the nodes data and references. A reference to a view can
/// be converted into a GraphPtr.
pub mod node_views {
    use super::*;
    #[repr(C)]
    pub struct NamedNode<'id, N, E> {
        pub refs : HashMap<GraphPtr<'id, super::NamedNode<N, E>>, E>,
        pub data : N,
        //must not contain ServiceData as it allows to corrupt the graph using mem::swap
    }
}

pub trait GraphNode : Sized {
    type Node;
    type Edge;
    fn get(&self) -> &Self::Node;
    fn get_mut(&mut self) -> &mut Self::Node;

    fn service(&self) -> &ServiceData;
    fn service_mut(&mut self) -> &mut ServiceData;

    //TODO: use SmallBox
    //TODO2: use impl Iterator when available or monomophise manually
    fn edge_ptrs<'a>(&'a self) -> Box<dyn Iterator<Item = *mut Self> + 'a>;
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

    fn service(&self) -> &ServiceData {
        &self.service
    }
    
    fn service_mut(&mut self) -> &mut ServiceData {
        &mut self.service
    }

    fn edge_ptrs<'a>(&'a self) -> Box<dyn Iterator<Item = *mut Self> + 'a> {
        Box::new(self.refs.iter().map(|x| { x.0.as_mut() }))
    }

    fn from_data(data : Self::Node) -> Self
    {
        let service = ServiceData { cleanup_gen : CleanupGen::Even, store_index : 0 };
        NamedNode { refs : HashMap::new(), data, service }
    }
}