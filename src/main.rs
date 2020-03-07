use generativity::*;
use std::collections::HashMap;
use std::collections::VecDeque;
use core::hash::{Hash, Hasher};
use core::marker::PhantomData;
use core::mem::size_of;
use core::mem::size_of_val;
use core::ops::{Index, IndexMut};

#[repr(transparent)]
#[derive(Eq)]
pub struct GraphRef<'id, T> {
    node : *mut T,
    _guard : Id<'id>
}

impl <'id, T> PartialEq for GraphRef<'id, T> {
    fn eq(&self, other : &Self) -> bool {
        self.node == other.node
    }
}

impl <'id, T> GraphRef<'id, T> {
    fn as_mut(self) -> *mut T {
        self.node as *mut T
    }

    pub fn as_raw(self) -> *const T {
        self.node
    }

    unsafe fn from_mut(ptr : *mut T, guard : Id<'id>) -> Self {
        GraphRef { node : ptr, _guard : guard }
    }

    unsafe fn from_raw(ptr : *const T, guard : Id<'id>) -> Self {
        GraphRef { node : ptr as *mut T, _guard : guard }
    }
}

impl <'id, T> Hash for GraphRef<'id, T>  {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.node.hash(state);
    }
}

impl <'id, T> Clone for GraphRef<'id, T> {
    fn clone(&self) -> GraphRef<'id, T> {
        unsafe {
            GraphRef::from_raw(self.node, self._guard )
        }
    }
}

impl <'id, T> Copy for GraphRef<'id, T> {}

#[derive(PartialEq, Eq)]
pub struct HashNode<N, E> {
    refs : HashMap<*const HashNode<N, E>, E>,
    payload : N,
}

enum GcGeneration {
    Even,
    Odd,
}

pub trait GraphNode : Sized {
    type Node;
    type Edge;
    fn get(&self) -> &Self::Node;
    fn get_mut(&mut self) -> &mut Self::Node;
    fn from_payload(data : Self::Node) -> Self;
}


pub struct EdgeBoth<N, E> {
    pub this : N,
    pub that : N,
    pub edge : E
}

pub struct EdgeLoop<N, E> {
    pub this : N,
    pub edge : E
}

pub enum EdgeOption<N, E> {
    Both(EdgeBoth<N, E>),
    Loop(EdgeLoop<N, E>),
    Neither
}

use crate::EdgeOption::Both;
use crate::EdgeOption::Loop;
use crate::EdgeOption::Neither;

impl <N, E> EdgeOption<N, E>{
    pub fn this(self) -> Option<(N, E)> {
        match self {
            Both(s) => Some((s.this, s.edge)),
            Loop(s) => Some((s.this, s.edge)),
            Neither => None,
        }
    }

    pub fn that(self) -> Option<(N, E)> {
        match self {
            Both(s) => Some((s.that, s.edge)),
            Loop(s) => Some((s.this, s.edge)),
            Neither => None,
        }
    }

    pub fn both(self) -> Option<EdgeBoth<N, E>> {
        match self {
            Both(s) => Some(s),
            _ => None,
        }
    }
}

impl <N, E> GraphNode for HashNode<N, E> {
    type Node = N;
    type Edge = E;

    fn get(&self) -> &Self::Node {
        &self.payload
    }

    fn get_mut(&mut self) -> &mut Self::Node {
        &mut self.payload
    }

    fn from_payload(data : Self::Node) -> Self {
        HashNode { refs : HashMap::new(), payload : data }
    }
}

pub struct GraphRaw<T> {
    data : Vec<Box<T>>
}

impl <N, E, NodeType : GraphNode<Node = N, Edge = E>> GraphRaw<NodeType> {
    fn spawn_detached(&mut self, payload : N) -> *const NodeType {
        let node = Box::new(NodeType::from_payload(payload));
        let ptr : *const NodeType = &*node;
        self.data.push(node);
        ptr
    }

    fn get<'id>(&self, item : GraphRef<'id, NodeType>) -> &N {
        unsafe {
            (*item.as_raw()).get()
        }
    }

    fn get_mut<'id>(&mut self, item : GraphRef<'id, NodeType>) -> &mut N {
        unsafe {
            (*item.as_mut()).get_mut()
        }
    }

    unsafe fn kill<'id>(&mut self, item : GraphRef<'id, NodeType>) {}
}

impl <N, E, NodeType : GraphNode<Node = N, Edge = E>> GraphRaw<NodeType> {
    fn bridge<'id>(&mut self, src : GraphRef<'id, NodeType>, dst : GraphRef<'id, NodeType>) -> EdgeOption<&'_ mut N, ()> {
        let this = self.get_mut(src);
        if src == dst {
            Loop(EdgeLoop { this : this, edge : () })
        } else {
            let that = unsafe { (*dst.as_mut()).get_mut() };
            Both(EdgeBoth { this : this, that : that, edge : () })
        }
    }
}

impl <N, E> GraphRaw<HashNode<N, E>> {
    fn connect<'id>(&mut self, source : GraphRef<'id, HashNode<N, E>>, dest : GraphRef<'id, HashNode<N, E>>, edge : E) {
        let refs = unsafe { &mut (*source.as_mut()).refs };
        refs.insert(dest.as_raw(), edge);
    }

    fn disconnect<'id>(&mut self, source : GraphRef<'id, HashNode<N, E>>, dest : GraphRef<'id, HashNode<N, E>>) {
        let refs = unsafe { &mut (*source.as_mut()).refs };
        refs.remove(&dest.as_raw());
    }

    fn get_edge<'id>(&self, src : GraphRef<'id, HashNode<N, E>>, dst : GraphRef<'id, HashNode<N, E>>) -> EdgeOption<&'_ N, &'_ E> {
        let this = unsafe { &(*src.as_raw()) };

        let this_refs = &this.refs;
        let this = &this.payload;
        
        if let Some(e) = this_refs.get(&dst.as_raw()) {
            if src == dst {
                Loop(EdgeLoop { this : this, edge : &e })
            } else {
                let that = self.get(dst);
                Both(EdgeBoth { this : this, that : that, edge : &e })
            }
        } else {
            Neither
        }
    }

    fn get_edge_mut<'id>(&mut self, src : GraphRef<'id, HashNode<N, E>>, dst : GraphRef<'id, HashNode<N, E>>) -> EdgeOption<&'_ mut N, &'_ mut E> {
        let this = unsafe { &mut (*src.as_mut()) };

        let this_refs = &mut this.refs;
        let this = &mut this.payload;
        
        if let Some(e) = this_refs.get_mut(&(dst.as_raw())) {
            if src == dst {
                Loop(EdgeLoop { this : this, edge : e })
            } else {
                let that = self.get_mut(dst);
                Both(EdgeBoth { this : this, that : that, edge : e })
            }
        } else {
            Neither
        }
    }
}

impl <T> GraphRaw<T> {
    fn new() -> GraphRaw<T> {
        GraphRaw { data : Vec::new() }
    }
}

pub struct VecGraph<T> {
    internal : GraphRaw<T>,
    root: Vec<*const T>,
}

impl <T> VecGraph<T> {
    pub fn new() -> VecGraph<T> {
        VecGraph { root : Vec::new(), internal : GraphRaw::new() }
    }
}

pub enum CleanupStrategy {
    Never,
}

pub struct AnchorMut<'this, 'id : 'this, T : 'this> {
    //Theorem Q: dereferencing GraphRef in every non-recursive function typed 
    //   (&'_ self, GraphRef<'id>, ...) -> &'_
    //   (&'_ mut self, GraphRef<'id>, ...) -> &'_ mut
    //   (&'_ mut self, GraphRef<'id>, ...) -> ()
    //is memory safe

    //(1) Graph nodes can only be deallocated when AnchorMut is dropped which in turn invalidates any GraphRef
    //(2) Every pointer is created from valid Box and pointer arithmetics is not used
    //(3) Mutable aliasing is impossible
        //(a) GraphRef can only be used with an Anchor or AnchorMut of the same 'id
        //(b) GraphRef cannot be dereferenced directly
        //(c) Consecutive calls of the functions will invalidate each others outputs as per Rust borrowing rules
    parent: &'this mut T,
    strategy : CleanupStrategy,
    _guard : Id<'id>
}


impl <'this, 'id: 'this, T : 'this> Drop for AnchorMut<'this, 'id, T> {
    fn drop(&mut self) {}
}


impl <'this, 'id : 'this, N : 'this, E : 'this,
      NodeType : 'this + GraphNode<Node = N, Edge = E>,
      Graph : 'this + GraphCommonMut<InternalGraph = GraphRaw<NodeType>>
> Index<GraphRef<'id, NodeType>> for AnchorMut<'this, 'id, Graph> {
    type Output = N;
    fn index(&self, target : GraphRef<'id, NodeType>) -> &Self::Output {
        self.internal().get(target)
    }
}

impl <'this, 'id : 'this, N : 'this, E : 'this,
      NodeType : 'this + GraphNode<Node = N, Edge = E>,
      Graph : 'this + GraphCommonMut<InternalGraph = GraphRaw<NodeType>>
> IndexMut<GraphRef<'id, NodeType>> for AnchorMut<'this, 'id, Graph> {
    fn index_mut(&mut self, target : GraphRef<'id, NodeType>) -> &mut Self::Output {
        self.internal_mut().get_mut(target)
    }
}

pub struct GraphIterRes<N, E, T> {
    pub ptr : T,
    pub node : N,
    pub edge : E
}

pub trait GraphCommon : Sized {
    type InternalGraph;
    fn internal(&self) -> &Self::InternalGraph;
}

pub trait GraphCommonMut : GraphCommon {
    fn internal_mut(&mut self) -> &mut Self::InternalGraph;

    fn anchor_mut<'id>(&mut self, guard : Id<'id>, strategy : CleanupStrategy) -> AnchorMut<'_, 'id, Self> {
        AnchorMut { parent : self, _guard : guard, strategy : strategy }
    }
}

impl <T> GraphCommon for VecGraph<T> {
    type InternalGraph = GraphRaw<T>;
    fn internal(&self) -> &Self::InternalGraph {
        &self.internal
    }
}

impl <T> GraphCommonMut for VecGraph<T> {
    fn internal_mut(&mut self) -> &mut Self::InternalGraph {
        &mut self.internal
    }
}

impl <'this, 'id : 'this, N : 'this, E : 'this,
      NodeType : 'this + GraphNode<Node = N, Edge = E>,
      Graph : 'this + GraphCommonMut<InternalGraph = GraphRaw<NodeType>>
> AnchorMut<'this, 'id, Graph> {

    fn internal(&self) -> &GraphRaw<NodeType> {
        self.parent.internal()
    }

    fn internal_mut(&mut self) -> &mut GraphRaw<NodeType> {
        self.parent.internal_mut()
    }

    pub fn spawn_detached(&mut self, payload : N) -> GraphRef<'id, NodeType> {
        let ptr = self.internal_mut().spawn_detached(payload);
        unsafe {
            GraphRef::from_raw(ptr, self._guard )
        }
    }

    pub unsafe fn ref_from_raw(&self, raw : *const NodeType) -> GraphRef<'id, NodeType> {
        GraphRef::from_raw(raw, self._guard)
    }

    pub fn cursor_mut(&mut self, target : GraphRef<'id, NodeType>) ->
                      CursorMut<'_, 'id, NodeType> {
        let g = self._guard;
        CursorMut { _guard : g, parent : self.internal_mut(), current : target.as_mut() }
    }

    pub fn cursor(&self, target : GraphRef<'id, NodeType>) ->
                      Cursor<'_, 'id, NodeType> {
        let g = self._guard;
        Cursor { _guard : g, parent : self.internal(), current : target.as_raw() }
    }
}

impl <'this, 'id : 'this, N : 'this, E : 'this,
      Graph : 'this + GraphCommonMut<InternalGraph = GraphRaw<HashNode<N, E>>>
> AnchorMut<'this, 'id, Graph> {
    pub fn connect(&mut self, source : GraphRef<'id, HashNode<N, E>>,
                   dest : GraphRef<'id, HashNode<N, E>>, edge : E)
    {
        self.internal_mut().connect(source, dest, edge);
    }

    pub fn disconnect(&mut self, source : GraphRef<'id, HashNode<N, E>>,
                      dest : GraphRef<'id, HashNode<N, E>>)
    {
        self.internal_mut().disconnect(source, dest);
    }
}

impl <'this, 'id : 'this, N : 'this, E : 'this, NodeType : 'this + GraphNode<Node = N, Edge = E>>
AnchorMut<'this, 'id, VecGraph<NodeType>>
{
    pub fn spawn(&mut self, payload : N) -> GraphRef<'id, NodeType> {
        let res = self.spawn_detached(payload);
        self.attach(res);
        res
    }

    pub fn attach(&mut self, node : GraphRef<'id, NodeType>) {
        self.parent.root.push(node.node);
    }

    pub fn detach(&mut self, index : usize) {
        self.parent.root.swap_remove(index);
    }

    pub fn iter(&self) -> impl Iterator<Item = GraphIterRes<&'_ N, (), GraphRef<'id, NodeType>>> {
        let g = self._guard;
        self.parent.root.iter().map(move |x| {
            let x = *x;
            let p =  unsafe { GraphRef::from_raw(x, g) };
            let payload = unsafe { (*x).get() };
            GraphIterRes { ptr : p, node : payload, edge : () }
        })
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = GraphIterRes<&'_ mut N, (), GraphRef<'id, NodeType>>> {
        let g = self._guard;
        self.parent.root.iter_mut().map(move |x| {
            let x = *x as *mut NodeType;
            let p =  unsafe { GraphRef::from_mut(x, g) };
            let payload = unsafe { (*x).get_mut() };
            GraphIterRes { ptr : p, node : payload, edge : () }
        })
    }
}

pub struct CursorMut<'this, 'id : 'this, T : 'this> {
    //(Q) applies due to Rust borrowing rules
    _guard : Id<'id>,
    parent : &'this mut GraphRaw<T>,
    current : *mut T
}

//Shared with Cursor
impl <'this, 'id : 'this, N : 'this, E : 'this, NodeType : 'this + GraphNode<Node = N, Edge = E>>
CursorMut<'this, 'id, NodeType>
{
    pub fn at(&self) -> GraphRef<'id, NodeType> {
        unsafe { GraphRef::from_mut(self.current, self._guard ) }
    }

    pub fn is_at(&self, target : GraphRef<'id, NodeType>) -> bool {
        target.as_raw() == self.current as *const NodeType
    }

    pub fn get(&self) -> &N {
        self.parent.get(self.at())
    }

    pub fn jump(&mut self, target : GraphRef<'id, NodeType>) {
        self.current = target.node as *mut NodeType;
    }
}

impl <'this, 'id : 'this, N : 'this, E : 'this>
CursorMut<'this, 'id, HashNode<N, E>>
{
    pub fn iter(&self) -> impl Iterator<Item = GraphIterRes<&'_ N, &'_ E, GraphRef<'id, HashNode<N, E>>>> {
        let current = self.current as *const HashNode<N, E>;
        let g = self._guard;

        let node_refs = unsafe { &(*current).refs };
        node_refs.iter().map(move |x| {
            let ptr = *(x.0);
            let p =  unsafe { GraphRef::from_raw(ptr, g) };
            let node = unsafe { (*ptr).get() };
            GraphIterRes { ptr : p, node : node, edge : x.1 }
        })
    }

    fn get_edge(&self, dst : GraphRef<'id, HashNode<N, E>>) -> EdgeOption<&'_ N, &'_ E> {
        self.parent.get_edge(self.at(), dst)
    }
}

//CursorMut exclusives
impl <'this, 'id : 'this, N : 'this, E : 'this, NodeType : 'this + GraphNode<Node = N, Edge = E>>
CursorMut<'this, 'id, NodeType>
{
    pub fn bridge(&mut self, target : GraphRef<'id, NodeType>) -> EdgeOption<&'_ mut N, ()> {
        self.parent.bridge(self.at(), target)
    }

    pub fn get_mut(&mut self) -> &mut N {
        self.parent.get_mut(self.at())
    }
}

impl <'this, 'id : 'this, N : 'this, E : 'this>
CursorMut<'this, 'id, HashNode<N, E>>
{
    pub fn iter_mut(&mut self) -> impl Iterator<Item = GraphIterRes<&'_ mut N, &'_ mut E, GraphRef<'id, HashNode<N, E>>>> {
        let current = self.current;
        let g = self._guard;

        let node_refs = unsafe { &mut (*current).refs };
        node_refs.iter_mut().map(move |x| {
            let ptr = *(x.0) as *mut HashNode<N, E>;
            let p =  unsafe { GraphRef::from_mut(ptr, g) };
            let node = unsafe { (*ptr).get_mut() };
            GraphIterRes { ptr : p, node : node, edge : x.1 }
        })
    }

    pub fn attach(&mut self, target : GraphRef<'id, HashNode<N, E>>, edge : E) {
        self.parent.connect(self.at(), target, edge);
    }

    pub fn detach(&mut self, target : GraphRef<'id, HashNode<N, E>>) {
        self.parent.disconnect(self.at(), target);

    }

    fn get_edge_mut(&mut self, dst : GraphRef<'id, HashNode<N, E>>) -> EdgeOption<&'_ mut N, &'_ mut E> {
        self.parent.get_edge_mut(self.at(), dst)
    }
}

pub struct Cursor<'this, 'id : 'this, T : 'this> {
    _guard : Id<'id>,
    parent : &'this GraphRaw<T>,
    current : *const T
}


impl <'this, 'id : 'this, N : 'this, E : 'this, NodeType : 'this + GraphNode<Node = N, Edge = E>>
Cursor<'this, 'id, NodeType>
{
    pub fn at(&self) -> GraphRef<'id, NodeType> {
        unsafe {
            GraphRef::from_raw(self.current, self._guard)
        }
    }

    pub fn is_at(&self, target : GraphRef<'id, NodeType>) -> bool {
        target.as_raw() == self.current as *const NodeType
    }

    pub fn get(&self) -> &N {
        self.parent.get(self.at())
    }

    pub fn jump(&mut self, target : GraphRef<'id, NodeType>) {
        self.current = target.node as *mut NodeType;
    }
}

impl <'this, 'id : 'this, N : 'this, E : 'this>
Cursor<'this, 'id, HashNode<N, E>>
{
    pub fn iter(&self) -> impl Iterator<Item = GraphIterRes<&'_ N, &'_ E, GraphRef<'id, HashNode<N, E>>>> {
        let current = self.current as *const HashNode<N, E>;
        let g = self._guard;

        let node_refs = unsafe { &(*current).refs };
        node_refs.iter().map(move |x| {
            let ptr = *(x.0);
            let p =  unsafe { GraphRef::from_raw(ptr, g) };
            let node = unsafe { (*ptr).get() };
            GraphIterRes { ptr : p, node : node, edge : x.1 }
        })
    }

    fn get_edge(&self, dst : GraphRef<'id, HashNode<N, E>>) -> EdgeOption<&'_ N, &'_ E> {
        self.parent.get_edge(self.at(), dst)
    }
}


struct BfsNode {
    key : i32,
    distance : i32
}


fn breadth_first_search(gr : &mut VecGraph<HashNode<BfsNode, ()>>) {
    make_guard!(g);
    let mut anchor = gr.anchor_mut(Id::from(g), CleanupStrategy::Never);
    let root =  {
        let mut iter = anchor.iter();
        iter.next().unwrap().ptr
    };
    
    let mut cursor = anchor.cursor_mut(root);

    cursor.get_mut().distance = 0;
    let mut queue = VecDeque::new();
    queue.push_back(root);

    while !queue.is_empty() {
        let q = queue.pop_front().unwrap();
        cursor.jump(q);
        println!("Visiting {}", cursor.get().key);

        let dist = cursor.get().distance;

        for i in cursor.iter_mut() {
            if i.node.distance == -1 {
                queue.push_back(i.ptr);
                i.node.distance = dist + 1;
                println!("Touching {} distance {}", i.node.key, i.node.distance);
            }
        }
    }
}

fn test_bfs() {
    let mut graph = VecGraph::<HashNode<BfsNode, ()>>::new();
    {
        make_guard!(g);
        let mut anchor = graph.anchor_mut(Id::from(g), CleanupStrategy::Never);
        
        let mut vec = Vec::new();
        //Thomas Cormen, Introduction to Algorithms 2e, pic. 22.3
        for i in 0..8 {
            vec.push(anchor.spawn_detached(BfsNode { key : i, distance : -1}));
        }
        anchor.attach(vec[0]);

        anchor.connect(vec[0], vec[1], ());
        anchor.connect(vec[1], vec[0], ());

        anchor.connect(vec[1], vec[2], ());
        anchor.connect(vec[2], vec[1], ());

        anchor.connect(vec[0], vec[3], ());
        anchor.connect(vec[3], vec[0], ());
        
        anchor.connect(vec[0], vec[3], ());
        anchor.connect(vec[3], vec[0], ());

        anchor.connect(vec[3], vec[4], ());
        anchor.connect(vec[4], vec[3], ());
        
        anchor.connect(vec[3], vec[5], ());
        anchor.connect(vec[5], vec[3], ());
        
        anchor.connect(vec[4], vec[5], ());
        anchor.connect(vec[5], vec[4], ());

        anchor.connect(vec[4], vec[6], ());
        anchor.connect(vec[6], vec[4], ());

        anchor.connect(vec[5], vec[6], ());
        anchor.connect(vec[6], vec[5], ());

        anchor.connect(vec[5], vec[7], ());
        anchor.connect(vec[7], vec[5], ());
        
        anchor.connect(vec[6], vec[7], ());
        anchor.connect(vec[7], vec[6], ());
    }
    breadth_first_search(&mut graph);
}

type BFRef<'id> = GraphRef<'id, HashNode<usize, usize>>;


fn bellman_ford<'id, 'a>(graph : &AnchorMut<'a, 'id, VecGraph<HashNode<usize, usize>>>, count : usize,
                         source : BFRef<'id>) -> HashMap::<BFRef<'id>, BFRef<'id>>
{
    let mut dist = HashMap::new();
    let mut path = HashMap::new();//(to;from)

    let mut cursor = graph.cursor(source);
    for i in cursor.iter() {
        dist.insert(i.ptr, *i.edge);
        path.insert(i.ptr, source);
    }
    dist.insert(source, 0);

    for _ in 0..count - 1 {
        let nodes : Vec<_> = dist.keys().map(|x| {*x}).collect();
        for i in nodes {
            cursor.jump(i);
            for j in cursor.iter() {
                if !dist.contains_key(&j.ptr) ||
                    dist[&j.ptr] > dist[&i] + j.edge {
                    path.insert(j.ptr, i);
                    dist.insert(j.ptr, dist[&i] + j.edge);
                }
            }
        }
    }
    path
}


fn print_bf_path<'id, 'a>(graph : &AnchorMut<'a, 'id, VecGraph<HashNode<usize, usize>>>,
                path : &HashMap::<BFRef<'id>, BFRef<'id>>,
                source : BFRef<'id>, target : BFRef<'id>) {
    let mut cursor = graph.cursor(target);
    if path.contains_key(&target) {
        let mut whole = 0;
        while !cursor.is_at(source) {
            let cur = cursor.at();
            let prev = path[&cur];

            let cur_key = *cursor.get();
            let prev_key = graph[prev];

            cursor.jump(prev);

            let len = cursor.get_edge(cur).this().unwrap().1;
            whole += len;
            println!("{} to {}, len {}", prev_key, cur_key, len);
        }
        println!("Length {}", whole);
    }
    println!("_________");
}
 
fn shortest_path_test(){
    let mut graph = VecGraph::new();
    make_guard!(g);
    let mut anchor = graph.anchor_mut(Id::from(g), CleanupStrategy::Never);
    //Thomas Cormen, Introduction to Algorithms 2e, pic. 24.6

    let source = anchor.spawn(0);

    let n1 = anchor.spawn(1);
    let n2 = anchor.spawn(2);

    let n3 = anchor.spawn(3);
    let n4 = anchor.spawn(4);

    anchor.connect(source, n1, 10);
    anchor.connect(source, n2, 5);

    anchor.connect(n1, n2, 2);
    anchor.connect(n1, n3, 1);
    
    anchor.connect(n2, n1, 3);
    anchor.connect(n2, n4, 2);
    anchor.connect(n2, n3, 9);

    anchor.connect(n4, n3, 6);
    anchor.connect(n4, source, 7);

    anchor.connect(n3, n4, 4);

    let path = bellman_ford(&anchor, 5, source);
    print_bf_path(&anchor, &path, source, n1);
    print_bf_path(&anchor, &path, source, n2);
    print_bf_path(&anchor, &path, source, n3);
    print_bf_path(&anchor, &path, source, n4);
}

fn main() {
    test_bfs();
    shortest_path_test();
}
