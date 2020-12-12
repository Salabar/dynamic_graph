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


/// Views into nodes allowing direct access to the nodes data and references.
pub mod node_views {
    use super::*;

    macro_rules! define_node_view {
        ($NodeType:ident, $Collection:ident) => {
            pub struct $NodeType<'id, N, E> {
                pub refs : $Collection<'id, super::$NodeType<N, E>, E>,
                pub data : N,
            }

            impl <'id, N, E> $NodeType<'id, N, E> {
                pub(crate) fn new(data : N) -> Self {
                    $NodeType { data, refs: $Collection::default() }
                }
            }
        }
    }

    define_node_view!{VecNode, NodeVec}
    define_node_view!{NamedNode, NodeNamedMap}
    define_node_view!{OptionNode, NodeOption}

    pub struct TreeNode<'id, K, N, E> {
        pub refs : NodeTreeMap<'id, K, super::TreeNode<K, N, E>, E>,
        pub data : N,
    }

    impl <'id, K : Ord, N, E> TreeNode<'id, K, N, E> {
        pub(crate) fn new(data : N) -> Self {
            TreeNode { data, refs: BTreeMap::default() }
        }
    }
}

macro_rules! impl_node_type {
    ($NodeType:ident) => {

        pub struct $NodeType<N, E> {
            pub(crate) internal: node_views::$NodeType<'static, N, E>,
            pub(crate) meta : MetaData,
        }

        impl <N, E> $NodeType<N, E> {
            pub (crate) fn get_view<'id>(&self) -> &node_views::$NodeType<'id, N, E> {
                unsafe {
                    transmute(&self.internal)
                }
            }

            pub (crate) fn get_view_mut<'id>(&mut self) -> &mut node_views::$NodeType<'id, N, E> {
                unsafe {
                    transmute(&mut self.internal)
                }
            }
        }

        impl <N, E> GraphNode for $NodeType<N, E> {
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
                Self { internal : node_views::$NodeType::new(data), meta }
            }
        }
    }
}

impl_node_type!{VecNode}
impl_node_type!{NamedNode}
impl_node_type!{OptionNode}

pub struct TreeNode<K, N, E> {
    pub(crate) internal: node_views::TreeNode<'static, K, N, E>,
    pub(crate) meta : MetaData,
}

impl <K, N, E> TreeNode<K, N, E> {
    pub (crate) fn get_view<'id>(&self) -> &node_views::TreeNode<'id, K, N, E> {
        unsafe {
            transmute(&self.internal)
        }
    }

    pub (crate) fn get_view_mut<'id>(&mut self) -> &mut node_views::TreeNode<'id, K, N, E> {
        unsafe {
            transmute(&mut self.internal)
        }
    }
}

impl <K : Ord, N, E> GraphNode for TreeNode<K, N, E> {
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
        Self { internal : node_views::TreeNode::new(data), meta }
    }
}

pub unsafe trait NodeCollection<'id, NodeType : GraphNode> : Default {
    fn traverse(this : &Self, cleanup : &mut CleanupState<NodeType>);
}

pub trait TrivialNodeCollection<'id, NodeType : GraphNode> : NodeCollection<'id, NodeType> {
    type Key;
    type Edge;

    fn _get(&self, key : Self::Key) -> Option<(GraphPtr<'id, NodeType>, &Self::Edge)>;
    fn _get_mut(&mut self, key : Self::Key) -> Option<(GraphPtr<'id, NodeType>, &mut Self::Edge)>;
}

pub unsafe trait RootCollection<'id, NodeType : GraphNode> : Default {
    fn traverse(this : &Self, cleanup : &mut CleanupState<NodeType>);
}

fn traverse_touch<NodeType : GraphNode>(iter : impl Iterator<Item = *mut NodeType>, cleanup : &mut CleanupState<NodeType>) {
    for i in iter {
        cleanup.touch(i);
    }
}

pub type RootVec<'id, T> = Vec<GraphPtr<'id, T>>;
pub type RootNamedSet<'id, T> = HashSet<GraphPtr<'id, T>>;
pub type RootOption<'id, T> = Option<GraphPtr<'id, T>>;
pub type RootHashMap<'id, K, T> = HashMap<K, GraphPtr<'id, T>>;

pub type NodeVec<'id, NodeType, E> = Vec<(GraphPtr<'id, NodeType>, E)>;
pub type NodeNamedMap<'id, NodeType, E> = HashMap<GraphPtr<'id, NodeType>, E>;
pub type NodeOption<'id, NodeType, E> = Option<(GraphPtr<'id, NodeType>, E)>;
pub type NodeTreeMap<'id, K, NodeType, E> = BTreeMap<K, (GraphPtr<'id, NodeType>, E)>;

macro_rules! impl_root_collection {
    ($collection:ident) => {
        unsafe impl <'id, NodeType> RootCollection<'id, NodeType> for $collection<'id, NodeType>
        where NodeType : GraphNode
        {
            fn traverse(this : &Self, cleanup : &mut CleanupState<NodeType>) {
                traverse_touch(this.iter().map(|x| x.as_mut()), cleanup);
            }
        }
    }
}


unsafe impl <'id, K, NodeType> RootCollection<'id, NodeType> for RootHashMap<'id, K, NodeType>
where NodeType : GraphNode,
      K : Hash + Eq
{
    fn traverse(this : &Self, cleanup : &mut CleanupState<NodeType>) {
        traverse_touch(this.values().map(|x| x.as_mut()), cleanup);
    }
}

impl_root_collection!{RootVec}
impl_root_collection!{RootNamedSet}
impl_root_collection!{RootOption}

macro_rules! impl_node_collection {
    ($collection:ident) => {
        unsafe impl <'id, NodeType, E> NodeCollection<'id, NodeType> for $collection<'id, NodeType, E>
        where NodeType : GraphNode
        {
            fn traverse(this : &Self, cleanup : &mut CleanupState<NodeType>) {
                traverse_touch(this.iter().map(|x| x.0.as_mut()), cleanup);
            }
        }
    }
}


impl_node_collection!{NodeVec}
impl_node_collection!{NodeNamedMap}
impl_node_collection!{NodeOption}

impl <'id, NodeType, E> TrivialNodeCollection<'id, NodeType> for NodeNamedMap<'id, NodeType, E>
where NodeType : GraphNode
{
    type Key = GraphPtr<'id, NodeType>;
    type Edge = E;

    fn _get(&self, key : Self::Key) -> Option<(GraphPtr<'id, NodeType>, &Self::Edge)> {
        self.get(&key).map(move |x| (key, x))
    }

    fn _get_mut(&mut self, key : Self::Key) -> Option<(GraphPtr<'id, NodeType>, &mut Self::Edge)> {
        self.get_mut(&key).map(move |x| (key, x))
    }
}

impl <'id, NodeType, E> TrivialNodeCollection<'id, NodeType> for NodeVec<'id, NodeType, E>
where NodeType : GraphNode
{
    type Key = usize;
    type Edge = E;

    fn _get(&self, key : Self::Key) -> Option<(GraphPtr<'id, NodeType>, &Self::Edge)> {
        self.get(key).map(|x| (x.0, &x.1))
    }

    fn _get_mut(&mut self, key : Self::Key) -> Option<(GraphPtr<'id, NodeType>, &mut Self::Edge)> {
        self.get_mut(key).map(|x| (x.0, &mut x.1))
    }
}




unsafe impl <'id, K, NodeType, E> NodeCollection<'id, NodeType> for NodeTreeMap<'id, K, NodeType, E>
where NodeType : GraphNode,
      K : Ord
{
    fn traverse(this : &Self, cleanup : &mut CleanupState<NodeType>) {
        traverse_touch(this.values().map(|x| x.0.as_mut()), cleanup);
    }
}
