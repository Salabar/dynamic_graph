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
pub struct GraphRef<'id, E, N> {
    node : *const GraphNode<E, N>,
    _guard : Id<'id>
}


impl <'id, E, N> GraphRef<'id, E, N> {
    fn to_mut(self) -> *mut GraphNode<E, N> {
        self.node as *mut GraphNode<E, N>
    }

    pub fn as_raw(self) -> *const GraphNode<E, N> {
        self.node
    }
}

impl <'id, E, N> Hash for GraphRef<'id, E, N>  {
    fn hash<H: Hasher>(&self, state: &mut H) {
        let ptr : usize = unsafe {
            transmute(self.node) 
        };
        ptr.hash(state);
    }
}

impl <'id, E, N> Clone for GraphRef<'id, E, N> {
    fn clone(&self) -> GraphRef<'id, E, N> {
        GraphRef { node : self.node, _guard : self._guard }
    }
}

impl <'id, E, N> Copy for GraphRef<'id, E, N> {}

pub struct GraphNode<E, N> {
    refs : HashMap<*const GraphNode<E, N>, E>,
    payload : N
}

impl <E, N> GraphNode<E, N> {
    fn from_payload(data : N) -> GraphNode<E, N> {
        GraphNode { refs : HashMap::new(), payload : data }
    }
}

pub struct Graph<E, N> {
    root: Vec<*const GraphNode<E, N>>,
    _ph: PhantomData<GraphNode<E, N>>
}

impl <E, N> Graph<E, N> {
    pub fn new() -> Graph<E, N> {
        Graph { root : Vec::new(), _ph : PhantomData }
    }
}

pub enum CleanupStrategy {
    Never,
}


pub struct AnchorMut<'this, 'id : 'this, E : 'this, N : 'this> {
    //Theorem Q: dereferencing GraphRef in every function typed 
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
    parent: &'this mut Graph<E, N>,
    strategy : CleanupStrategy,
    _guard : Id<'id>
}


impl <'this, 'id: 'this, E : 'this, N : 'this> Drop for AnchorMut<'this, 'id, E, N> {
    fn drop(&mut self){}
}

impl <'id, E, N> Graph<E, N> {
    pub fn anchor_mut(&mut self, guard : Id<'id>, strategy : CleanupStrategy) -> AnchorMut<'_, 'id, E, N> {
        AnchorMut { parent : self, _guard : guard, strategy : strategy }
    }
}


impl <'this, 'id : 'this, E : 'this, N : 'this> Index<GraphRef<'id, E, N>> for AnchorMut<'this, 'id, E, N> {
    type Output = N;
    fn index(&self, target : GraphRef<'id, E, N>) -> &Self::Output {
        //(Q)
        let that =  unsafe {
            &*target.node
        };
        &that.payload
    }
}

impl <'this, 'id : 'this, E : 'this, N : 'this> IndexMut<GraphRef<'id, E, N>> for AnchorMut<'this, 'id, E, N> {
    fn index_mut(&mut self, target : GraphRef<'id, E, N>) -> &mut Self::Output {
        //(Q)
        let that =  unsafe {
            &mut *target.to_mut()
        };
        &mut that.payload
    }
}

impl <'this, 'id : 'this, E : 'this, N : 'this> AnchorMut<'this, 'id, E, N> {
    pub fn spawn_detached(&mut self, payload : N) -> GraphRef<'id, E, N> {
        let node = Box::new(GraphNode::from_payload(payload));
        GraphRef { _guard : self._guard, node : Box::into_raw(node) }
    }

    pub fn spawn(&mut self, payload : N) -> GraphRef<'id, E, N> {
        let res = self.spawn_detached(payload);
        self.attach(res);
        res
    }

    pub fn attach(&mut self, node : GraphRef<'id, E, N>) {
        self.parent.root.push(node.node);
    }

    pub fn detach(&mut self, index : usize) {
        self.parent.root.swap_remove(index);
    }

    pub fn connect(&mut self, source : GraphRef<'id, E, N>, dest : GraphRef<'id, E, N>, edge : E) {
        let ptr = source.to_mut();
        //(Q)
        let refs = unsafe { &mut (*ptr).refs };
        refs.insert(dest.node, edge);
    }

    pub fn disconnect(&mut self, source : GraphRef<'id, E, N>, dest : GraphRef<'id, E, N>) {
        let ptr = source.to_mut();
        //(Q)
        let refs = unsafe { &mut (*ptr).refs };
        refs.remove(&dest.node);
    }

    pub fn iter(&self) -> impl Iterator<Item = RootIterRes<'_, 'id, E, N>> {
        let g = self._guard;
        self.parent.root.iter().map(move |x| {
            let x = *x;
            let p =  GraphRef { node : x, _guard : g };
            let payload = unsafe { &(*x).payload };
            RootIterRes { ptr : p, node : payload }
        })
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = RootIterMutRes<'_, 'id, E, N>> {
        let g = self._guard;
        self.parent.root.iter_mut().map(move |x| {
            let x = *x;
            let p =  GraphRef { node : x, _guard : g };
            let x = x as *mut GraphNode<E, N>;
            let payload = unsafe { &mut (*x).payload };
            RootIterMutRes { ptr : p, node : payload }
        })
    }

    pub fn cursor_mut(&mut self, target : GraphRef<'id, E, N>) -> CursorMut<'_, 'this, 'id, E, N> {
        CursorMut { anchor : self, current : target.to_mut() }
    }

    pub fn cursor(&self, target : GraphRef<'id, E, N>) -> Cursor<'_, 'this, 'id, E, N> {
        Cursor { anchor : self, current : target.node }
    }
}

pub struct IterRes<'this, 'id : 'this, E : 'this, N : 'this> {
    ptr : GraphRef<'id, E, N>,
    node : &'this N,
    edge : &'this E
}

pub struct RootIterRes<'this, 'id : 'this, E : 'this, N : 'this> {
    ptr : GraphRef<'id, E, N>,
    node : &'this N,
}

pub struct IterMutRes<'this, 'id : 'this, E : 'this, N : 'this> {
    ptr : GraphRef<'id, E, N>,
    node : &'this mut N,
    edge : &'this mut E
}

pub struct RootIterMutRes<'this, 'id : 'this, E : 'this, N : 'this> {
    ptr : GraphRef<'id, E, N>,
    node : &'this mut N,
}

pub struct CursorMut<'this, 'anchor : 'this, 'id : 'anchor, E : 'this, N : 'this> {
    anchor : &'this mut AnchorMut<'anchor, 'id, E, N>,
    current : *mut GraphNode<E, N>
}
/*
trait CursorTrait<'a, 'anchor : 'a, 'id : 'anchor, E : 'a, N : 'a> {
    fn iter<Iter : 'a>(&'a self) -> Iter where Iter : Iterator<Item = IterRes<'a, 'id, E, N>>;
}
impl <'a, 'this : 'a, 'anchor : 'this, 'id: 'anchor, E : 'this, N : 'this> CursorTrait<'a, 'anchor, 'id, E, N>
for CursorMut<'this, 'anchor, 'id, E, N> {
    fn iter<Iter : 'a>(&'a self) -> Iter where Iter : Iterator<Item = IterRes<'a, 'id, E, N>> {        let current = self.current as *const GraphNode<E, N>;
        let node = unsafe { &(*current) };
        node.refs.iter().map(|x| {
            //(W)
            let ptr = *(x.0);
            let p =  unsafe { GraphRef { node : ptr, _guard : Id::new() } };
            let node = unsafe { &(*ptr) };
            let payload = &node.payload;
            IterRes { ptr : p, node : payload, edge : x.1 }
        })
    }
}
*/
impl <'this, 'anchor : 'this, 'id : 'anchor, E : 'this, N : 'this >
    CursorMut<'this, 'anchor, 'id, E, N> {

    pub fn iter(&self) -> impl Iterator<Item = IterRes<'_, 'id, E, N>> {
        let current = self.current as *const GraphNode<E, N>;
        let node = unsafe { &(*current) };
        let g = self.anchor._guard;
        node.refs.iter().map(move |x| {
            let ptr = *(x.0);
            let p =  GraphRef { node : ptr, _guard : g };
            let node = unsafe { &(*ptr) };
            let payload = &node.payload;
            IterRes { ptr : p, node : payload, edge : x.1 }
        })
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = IterMutRes<'_, 'id, E, N>> {
        let current = self.current;
        //this reference is dropped before closure below is ever executed and therefore
        //there is no aliasing
        let node = unsafe { &mut (*current) };
        let g = self.anchor._guard;
        node.refs.iter_mut().map(move |x| {
            let ptr = *(x.0) as *mut GraphNode<E, N>;
            let p =  GraphRef { node : ptr, _guard : g };
            let node = unsafe { &mut (*ptr) };
            let payload = &mut node.payload;
            IterMutRes { ptr : p, node : payload, edge : x.1 }
        })
    }

    pub fn at(&self) -> GraphRef<'id, E, N> {
        GraphRef { node : self.current as *const GraphNode<E, N>, _guard : self.anchor._guard }
    }

    pub fn is_at(&self, target : GraphRef<'id, E, N>) -> bool {
        target.node == self.current as *const GraphNode<E, N>
    }

    pub fn get(&self) -> &N {
        let this = self.at();
        &self.anchor[this]
    }

    pub fn get_mut(&mut self) -> &mut N {
        let this = self.at();
        &mut self.anchor[this]
    }
    
    pub fn bridge(&mut self, target : GraphRef<'id, E, N>) -> Option<(&mut N, &mut N)> {
        let that = target.to_mut();
        if self.current != that {
            //(Q) applies due to Rust borrowing rules
            //Branch condition guarantees there is no aliasing
            let this = unsafe { &mut (*self.current).payload };
            let that = &mut self.anchor[target];
            Some((this, that))
        } else {
            None
        }
    }

    pub fn get_edge(&self, target : GraphRef<'id, E, N>) -> Option<IterRes<'_, 'id, E, N>> {
        let this = self.current as *const GraphNode<E, N>;
        let this = unsafe { &*this };
        if let Some(e) = this.refs.get(&target.node) {
            let node = &self.anchor[target];
            Some(IterRes { ptr : target, edge : e, node : node})
        } else {
            None
        }
    }

    pub fn get_edge_mut(&mut self, target : GraphRef<'id, E, N>) -> Option<IterMutRes<'_, 'id, E, N>> {
        let this = self.current as *mut GraphNode<E, N>;
        let this = unsafe { &mut *this };
        //(Q)
        if let Some(e) = this.refs.get_mut(&target.node) {
            let node = unsafe { &mut (*target.to_mut()).payload };
            Some(IterMutRes { ptr : target, edge : e, node : node})
        } else {
            None
        }
    }

    pub fn attach(&mut self, target : GraphRef<'id, E, N>, edge : E) {
        let this = self.at();
        self.anchor.connect(this, target, edge);
    }

    pub fn detach(&mut self, target : GraphRef<'id, E, N>, edge : E) {
        let this = self.at();
        self.anchor.disconnect(this, target);
    }

    pub fn attach_to(&mut self, target : GraphRef<'id, E, N>, edge : E) {
        let this = self.at();
        self.anchor.connect(target, this, edge);
    }

    pub fn detach_from(&mut self, target : GraphRef<'id, E, N>) {
        let this = self.at();
        self.anchor.disconnect(target, this);
    }

    pub fn jump(&mut self, target : GraphRef<'id, E, N>) {
        self.current = target.node as *mut GraphNode<E, N>;
    }

    

}

/////////TODO: Utilize traits and/or macros to remove duplication
pub struct Cursor<'this, 'anchor : 'this, 'id : 'anchor, E : 'this, N : 'this> {
    anchor : &'this AnchorMut<'anchor, 'id, E, N>,
    current : *const GraphNode<E, N>
}

impl <'this, 'anchor : 'this, 'id : 'anchor, E : 'this, N : 'this >
    Cursor<'this, 'anchor, 'id, E, N> {

    pub fn iter(&self) -> impl Iterator<Item = IterRes<'_, 'id, E, N>> {
        let current = self.current as *const GraphNode<E, N>;
        let node = unsafe { &(*current) };
        let g = self.anchor._guard;
        node.refs.iter().map(move|x| {
            let ptr = *(x.0);
            let p =  GraphRef { node : ptr, _guard : g };
            let node = unsafe { &(*ptr) };
            let payload = &node.payload;
            IterRes { ptr : p, node : payload, edge : x.1 }
        })
    }

    pub fn at(&self) -> GraphRef<'id, E, N> {
        GraphRef { node : self.current as *const GraphNode<E, N>, _guard : self.anchor._guard }
    }

    pub fn is_at(&self, target : GraphRef<'id, E, N>) -> bool {
        target.node == self.current as *const GraphNode<E, N>
    }

    pub fn get(&self) -> &N {
        let this = self.at();
        &self.anchor[this]
    }

    pub fn jump(&mut self, target : GraphRef<'id, E, N>) {
        self.current = target.node;
    }

    pub fn get_edge(&self, target : GraphRef<'id, E, N>) -> Option<IterRes<'_, 'id, E, N>> {
        let this = self.current as *const GraphNode<E, N>;
        let this = unsafe { &*this };
        if let Some(e) = this.refs.get(&target.node) {
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


fn breadth_first_search(gr : &mut Graph<(), BfsNode>) {
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
    let mut graph = Graph::<(), BfsNode>::new();
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