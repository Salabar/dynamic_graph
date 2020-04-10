use core::hint::unreachable_unchecked;

pub struct GraphIterRes<E, T> {
    /// Edge data.
    pub values : E,
    /// A pointer to the node.
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

pub struct EdgeLoop<N, E> {
    ///Value from the node.
    pub this : N,
    ///Value from the edge.
    pub edge : E
}

///View into nodes data connected by an edge. Both if the edge connects two different nodes and Loop if the edge loops back to the source node.
pub enum Edge<N, E> {
    Both(EdgeBoth<N, E>),
    Loop(EdgeLoop<N, E>),
}

pub use crate::Edge::Both;
pub use crate::Edge::Loop;

/// An add-on to Option to make Edge interfacing with std more natural.
pub trait OptionEdge<N, E> {
    fn this(self) -> Option<EdgeLoop<N, E>>;
    fn that(self) -> Option<EdgeLoop<N, E>>;
    fn both(self) -> Option<EdgeBoth<N, E>>;
    fn edge(self) -> Option<E>;
    unsafe fn both_unchecked(self) -> Option<EdgeBoth<N, E>>;
}

impl <N, E> OptionEdge<N, E> for Option<Edge<N, E>>
{
    fn this(self) -> Option<EdgeLoop<N, E>> {
        self.map(|x| {
            x.this()
        })
    }

    fn that(self) -> Option<EdgeLoop<N, E>> {
        self.map(|x| {
            x.that()
        })
    }

    fn both(self) -> Option<EdgeBoth<N, E>> {
        match self {
            Some(s) => s.both(),
            _ => None,
        }
    }

    unsafe fn both_unchecked(self) -> Option<EdgeBoth<N, E>> {
        self.map(|x| {
            x.both_unchecked()
        })
    }

    fn edge(self) -> Option<E> {
        self.map(|x| {
            x.edge()
        })
    }
}

impl <N, E> Edge<N, E> {
    ///Returns data from the source node and the edge.
    pub fn this(self) -> EdgeLoop<N, E>
    {
        match self {
            Both(s) => EdgeLoop { this : s.this, edge : s.edge },
            Loop(s) => s,
        }
    }

    ///Returns data from the destination node and the edge.
    pub fn that(self) -> EdgeLoop<N, E>
    {
        match self {
            Both(s) => EdgeLoop { this : s.that, edge : s.edge },
            Loop(s) => s,
        }
    }

    ///Returns data from both nodes and the edge. Returns None if self is a Loop.
    pub fn both(self) -> Option<EdgeBoth<N, E>>
    {
        match self {
            Both(s) => Some(s),
            _ => None,
        }
    }

    /// Returns data from both nodes and the edge.
    /// # Safety
    /// Caller must guarantee value of self to be Both.
    pub unsafe fn both_unchecked(self) -> EdgeBoth<N, E>
    {
        match self {
            Both(s) => s,
            _ => unreachable_unchecked(),
        }
    }

    /// Returns the edge data.
    pub fn edge(self) -> E
    {
        self.this().edge
    }
}