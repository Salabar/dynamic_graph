use generativity::*;
use std::collections::HashMap;
use std::collections::VecDeque;
use std::collections::hash_map::Entry::Occupied;
use core::hash::{Hash, Hasher};
use core::mem::transmute;
use core::marker::PhantomData;
use core::mem::size_of;
use core::mem::size_of_val;
use core::ops::{Index, IndexMut};
#[derive(PartialEq, Eq, Debug)]
pub struct GraphRef<'id, N, E> {
    node : *const GraphNode<N, E>,
    _guard : Id<'id>
}


impl <'id, N, E> GraphRef<'id, N, E> {
    fn to_mut(self) -> *mut GraphNode<N, E> {
        self.node as *mut GraphNode<N, E>
    }

    pub fn as_raw(self) -> *const GraphNode<N, E> {
        self.node
    }
}

impl <'id, N, E> Hash for GraphRef<'id, N, E>  {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.node.hash(state);
    }
}

impl <'id, N, E> Clone for GraphRef<'id, N, E> {
    fn clone(&self) -> GraphRef<'id, N, E> {
        GraphRef { node : self.node, _guard : self._guard }
    }
}

impl <'id, N, E> Copy for GraphRef<'id, N, E> {}

pub struct GraphNode<N, E> {
    refs : HashMap<*const GraphNode<N, E>, E>,
    payload : N
}

impl <N, E> GraphNode<N, E> {
    fn from_payload(data : N) -> GraphNode<N, E> {
        GraphNode { refs : HashMap::new(), payload : data }
    }
}

pub struct Graph<N, E> {
    root: Vec<*const GraphNode<N, E>>,
    _ph: PhantomData<GraphNode<N, E>>
}

impl <N, E> Graph<N, E> {
    pub fn new() -> Graph<N, E> {
        Graph { root : Vec::new(), _ph : PhantomData }
    }
}

pub enum CleanupStrategy {
    Never,
}


pub struct AnchorMut<'this, 'id : 'this, N : 'this, E : 'this> {
    //Theorem Q: dereferencing GraphRef in every non-recursive function typed 
    //   (&'_ self, GraphRef<'id>, ...) -> &_
    //   (&'_mut self, GraphRef<'id>, ...) -> &'_mut
    //   (&'_mut self, GraphRef<'id>, ..). -> ()
    //is memory safe

    //(1) Graph nodes can only be deallocated when AnchorMut is dropped which in turn invalidates any GraphRef
    //(2) Every pointer is created from valid Box and pointer arithmetics is not used
    //(3) Mutable aliasing is impossible
        //(a) GraphRef can only be used with an Anchor or AnchorMut of the same 'id
        //(b) GraphRef cannot be dereferenced directly
        //(c) Consecutive calls of the functions will invalidate each others outputs as per Rust borrowing rules
    parent: &'this mut Graph<N, E>,
    strategy : CleanupStrategy,
    _guard : Id<'id>
}


impl <'this, 'id: 'this, N : 'this, E : 'this> Drop for AnchorMut<'this, 'id, N, E> {
    fn drop(&mut self){}
}

impl <'id, N, E> Graph<N, E> {
    pub fn anchor_mut(&mut self, guard : Id<'id>, strategy : CleanupStrategy) -> AnchorMut<'_, 'id, N, E> {
        AnchorMut { parent : self, _guard : guard, strategy : strategy }
    }
}


impl <'this, 'id : 'this, N : 'this, E : 'this> Index<GraphRef<'id, N, E>> for AnchorMut<'this, 'id, N, E> {
    type Output = N;
    fn index(&self, target : GraphRef<'id, N, E>) -> &Self::Output {
        //(Q)
        let that =  unsafe { &*target.node };
        &that.payload
    }
}

impl <'this, 'id : 'this, N : 'this, E : 'this> IndexMut<GraphRef<'id, N, E>> for AnchorMut<'this, 'id, N, E> {
    fn index_mut(&mut self, target : GraphRef<'id, N, E>) -> &mut Self::Output {
        //(Q)
        let that =  unsafe { &mut *target.to_mut() };
        &mut that.payload
    }
}

impl <'this, 'id : 'this, N : 'this, E : 'this> AnchorMut<'this, 'id, N, E> {
    pub fn spawn_detached(&mut self, payload : N) -> GraphRef<'id, N, E> {
        let node = Box::new(GraphNode::from_payload(payload));
        GraphRef { _guard : self._guard, node : Box::into_raw(node) }
    }

    pub fn spawn(&mut self, payload : N) -> GraphRef<'id, N, E> {
        let res = self.spawn_detached(payload);
        self.attach(res);
        res
    }

    pub fn attach(&mut self, node : GraphRef<'id, N, E>) {
        self.parent.root.push(node.node);
    }

    pub fn detach(&mut self, index : usize) {
        self.parent.root.swap_remove(index);
    }

    pub fn connect(&mut self, source : GraphRef<'id, N, E>, dest : GraphRef<'id, N, E>, edge : E) {
        let ptr = source.to_mut();
        //(Q)
        let refs = unsafe { &mut (*ptr).refs };
        refs.insert(dest.node, edge);
    }

    pub fn disconnect(&mut self, source : GraphRef<'id, N, E>, dest : GraphRef<'id, N, E>) {
        let ptr = source.to_mut();
        //(Q)
        let refs = unsafe { &mut (*ptr).refs };
        refs.remove(&dest.node);
    }

    pub fn iter(&self) -> impl Iterator<Item = RootIterRes<'_, 'id, N, E>> {
        let g = self._guard;
        self.parent.root.iter().map(move |x| {
            let x = *x;
            let p =  GraphRef { node : x, _guard : g };
            let payload = unsafe { &(*x).payload };
            RootIterRes { ptr : p, node : payload }
        })
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = RootIterMutRes<'_, 'id, N, E>> {
        let g = self._guard;
        self.parent.root.iter_mut().map(move |x| {
            let x = *x;
            let p =  GraphRef { node : x, _guard : g };
            let x = x as *mut GraphNode<N, E>;
            let payload = unsafe { &mut (*x).payload };
            RootIterMutRes { ptr : p, node : payload }
        })
    }

    pub fn cursor_mut(&mut self, target : GraphRef<'id, N, E>) -> CursorMut<'_, 'this, 'id, N, E> {
        CursorMut { anchor : self, current : target.to_mut() }
    }

    pub fn cursor(&self, target : GraphRef<'id, N, E>) -> Cursor<'_, 'this, 'id, N, E> {
        Cursor { anchor : self, current : target.node }
    }
}

pub struct IterRes<'this, 'id : 'this, N : 'this, E : 'this> {
    ptr : GraphRef<'id, N, E>,
    node : &'this N,
    edge : &'this E
}

pub struct RootIterRes<'this, 'id : 'this, N : 'this, E : 'this> {
    ptr : GraphRef<'id, N, E>,
    node : &'this N,
}

pub struct IterMutRes<'this, 'id : 'this, N : 'this, E : 'this> {
    ptr : GraphRef<'id, N, E>,
    node : &'this mut N,
    edge : &'this mut E
}

pub struct RootIterMutRes<'this, 'id : 'this, N : 'this, E : 'this> {
    ptr : GraphRef<'id, N, E>,
    node : &'this mut N,
}

pub struct CursorMut<'this, 'anchor : 'this, 'id : 'anchor, N : 'this, E : 'this> {
    //(Q) applies due to Rust borrowing rules
    anchor : &'this mut AnchorMut<'anchor, 'id, N, E>,
    current : *mut GraphNode<N, E>
}


impl <'this, 'anchor : 'this, 'id : 'anchor, N : 'this, E : 'this >
CursorMut<'this, 'anchor, 'id, N, E> {

    pub fn iter(&self) -> impl Iterator<Item = IterRes<'_, 'id, N, E>> {
        let current = self.current as *const GraphNode<N, E>;
        let node_refs = unsafe { &(*current).refs };
        let g = self.anchor._guard;
        node_refs.iter().map(move |x| {
            let ptr = *(x.0);
            let p =  GraphRef { node : ptr, _guard : g };
            let node = unsafe { &(*ptr) };
            let payload = &node.payload;
            IterRes { ptr : p, node : payload, edge : x.1 }
        })
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = IterMutRes<'_, 'id, N, E>> {
        let current = self.current;
        let node_refs = unsafe { &mut (*current).refs };
        let g = self.anchor._guard;
        node_refs.iter_mut().map(move |x| {
            let ptr = *(x.0) as *mut GraphNode<N, E>;
            let p =  GraphRef { node : ptr, _guard : g };
            let node = unsafe { &mut (*ptr) };
            let payload = &mut node.payload;
            IterMutRes { ptr : p, node : payload, edge : x.1 }
        })
    }

    pub fn at(&self) -> GraphRef<'id, N, E> {
        GraphRef { node : self.current as *const GraphNode<N, E>, _guard : self.anchor._guard }
    }

    pub fn is_at(&self, target : GraphRef<'id, N, E>) -> bool {
        target.node == self.current as *const GraphNode<N, E>
    }

    pub fn get(&self) -> &N {
        let this = self.at();
        &self.anchor[this]
    }

    pub fn get_mut(&mut self) -> &mut N {
        let this = self.at();
        &mut self.anchor[this]
    }
    
    pub fn bridge(&mut self, target : GraphRef<'id, N, E>) -> Option<(&mut N, &mut N)> {
        let that = target.to_mut();
        if self.current != that {
            //Branch condition guarantees there is no aliasing
            let this = unsafe { &mut (*self.current).payload };
            let that = &mut self.anchor[target];
            Some((this, that))
        } else {
            None
        }
    }

    pub fn get_edge(&self, target : GraphRef<'id, N, E>) -> Option<IterRes<'_, 'id, N, E>> {
        let this = self.current as *const GraphNode<N, E>;
        let this_refs = unsafe { &(*this).refs };
        if let Some(e) = this_refs.get(&target.node) {
            let node = &self.anchor[target];
            Some(IterRes { ptr : target, edge : e, node : node})
        } else {
            None
        }
    }

    pub fn get_edge_mut(&mut self, target : GraphRef<'id, N, E>) -> Option<IterMutRes<'_, 'id, N, E>> {
        let this = self.current as *mut GraphNode<N, E>;
        let this_refs = unsafe { &mut (*this).refs };
        //(Q)
        if let Some(e) = this_refs.get_mut(&target.node) {
            let node = unsafe { &mut (*target.to_mut()).payload };
            Some(IterMutRes { ptr : target, edge : e, node : node})
        } else {
            None
        }
    }

    pub fn attach(&mut self, target : GraphRef<'id, N, E>, edge : E) {
        let this = self.at();
        self.anchor.connect(this, target, edge);
    }

    pub fn detach(&mut self, target : GraphRef<'id, N, E>, edge : E) {
        let this = self.at();
        self.anchor.disconnect(this, target);
    }

    pub fn attach_to(&mut self, target : GraphRef<'id, N, E>, edge : E) {
        let this = self.at();
        self.anchor.connect(target, this, edge);
    }

    pub fn detach_from(&mut self, target : GraphRef<'id, N, E>) {
        let this = self.at();
        self.anchor.disconnect(target, this);
    }

    pub fn jump(&mut self, target : GraphRef<'id, N, E>) {
        self.current = target.node as *mut GraphNode<N, E>;
    }

}

/////////TODO: Utilize traits and/or macros to remove duplication
pub struct Cursor<'this, 'anchor : 'this, 'id : 'anchor, N : 'this, E : 'this> {
    anchor : &'this AnchorMut<'anchor, 'id, N, E>,
    current : *const GraphNode<N, E>
}

impl <'this, 'anchor : 'this, 'id : 'anchor, N : 'this, E : 'this >
Cursor<'this, 'anchor, 'id, N, E> {

    pub fn iter(&self) -> impl Iterator<Item = IterRes<'_, 'id, N, E>> {
        let current = self.current as *const GraphNode<N, E>;
        let node_refs = unsafe { &(*current).refs };
        let g = self.anchor._guard;
        node_refs.iter().map(move|x| {
            let ptr = *(x.0);
            let p =  GraphRef { node : ptr, _guard : g };
            let node = unsafe { &(*ptr) };
            let payload = &node.payload;
            IterRes { ptr : p, node : payload, edge : x.1 }
        })
    }

    pub fn at(&self) -> GraphRef<'id, N, E> {
        GraphRef { node : self.current as *const GraphNode<N, E>, _guard : self.anchor._guard }
    }

    pub fn is_at(&self, target : GraphRef<'id, N, E>) -> bool {
        target.node == self.current as *const GraphNode<N, E>
    }

    pub fn get(&self) -> &N {
        let this = self.at();
        &self.anchor[this]
    }

    pub fn jump(&mut self, target : GraphRef<'id, N, E>) {
        self.current = target.node;
    }

    pub fn get_edge(&self, target : GraphRef<'id, N, E>) -> Option<IterRes<'_, 'id, N, E>> {
        let this = self.current as *const GraphNode<N, E>;
        let this_refs = unsafe { &(*this).refs };
        if let Some(e) = this_refs.get(&target.node) {
            let node = unsafe { &(*target.node).payload };
            Some(IterRes { ptr : target, edge : e, node : node})
        } else {
            None
        }
    }
}

struct BfsNode {
    key : i32,
    distance : i32
}


fn breadth_first_search(gr : &mut Graph<BfsNode, ()>) {
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
    let mut graph = Graph::<BfsNode, ()>::new();
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

type BFRef<'id> = GraphRef<'id, usize, usize>;


fn bellman_ford<'id, 'a>(graph : &AnchorMut<'a, 'id, usize, usize>, count : usize,
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


fn print_bf_path<'id, 'a>(graph : &AnchorMut<'a, 'id, usize, usize>,
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

            let len = cursor.get_edge(cur).unwrap().edge;
            whole += len;
            println!("{} to {}, len {}", prev_key, cur_key, len);
        }
        println!("Length {}", whole);
    }
    println!("_________");
}

fn shortest_path_test(){
    let mut graph = Graph::new();
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