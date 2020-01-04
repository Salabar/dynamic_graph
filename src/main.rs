use generativity::*;
use std::collections::HashMap;
use core::hash::{Hash, Hasher};
use core::mem::transmute;
use core::marker::PhantomData;
use core::mem::size_of;
use core::mem::size_of_val;

#[derive(PartialEq, Eq, Debug)]
pub struct GraphRef<'this, 'id : 'this, E, N> {
    node : *const GraphNode<E, N>,
    _guard : PhantomData<&'this Guard<'id>>
}

impl <'this, 'id : 'this, E, N> Hash for GraphRef<'this, 'id, E, N>  {
    fn hash<H: Hasher>(&self, state: &mut H) {
        let ptr : usize = unsafe {
            transmute(self.node) 
        };
        ptr.hash(state);
    }
}

impl <'this, 'id : 'this, E, N> Clone for GraphRef<'this, 'id, E, N> {
    fn clone(&self) -> GraphRef<'this, 'id, E, N> {
        GraphRef { node : self.node, _guard : self._guard }
    }
}

impl <'this, 'id : 'this, E, N> Copy for GraphRef<'this, 'id, E, N> {}

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
    After
}

pub struct AnchorMut<'this, 'id : 'this, E, N> {
    parent: &'this mut Graph<E, N>,
    strategy : CleanupStrategy,
    _guard : &'this Guard<'id>
}

impl <'this, 'id : 'this, E, N> Graph<E, N> {
    pub fn anchor_mut(&'this mut self, guard : &'this Guard<'id>, strategy : CleanupStrategy) -> AnchorMut<'this, 'id, E, N> {
        AnchorMut { parent : self, _guard : guard, strategy : strategy }
    }
}

impl <'this, 'id : 'this, E, N> AnchorMut<'this, 'id, E, N> {
    pub fn new_detached(&mut self, payload : N) -> GraphRef<'this, 'id, E, N> {
        let node = Box::new(GraphNode::from_payload(payload));
        GraphRef { _guard : PhantomData, node : Box::into_raw(node) }
    }

    pub fn new(&mut self, payload : N) -> GraphRef<'this, 'id, E, N> {
        let res = self.new_detached(payload);
        self.attach(res);
        res
    }

    pub fn attach(&mut self, node : GraphRef<'this, 'id, E, N>) {
        self.parent.root.push(node.node);
    }

    pub fn detach(&mut self, index : usize) {
        self.parent.root.swap_remove(index);
    }

    pub fn connect(&mut self, source : GraphRef<'this, 'id, E, N>, dest : GraphRef<'this, 'id, E, N>, edge : E) {
        let ptr = source.node as *mut GraphNode<E, N>;
        let refs = unsafe { &mut (*ptr).refs };
        refs.insert(dest.node, edge);
    }

    pub fn disconnect(&mut self, source : GraphRef<'this, 'id, E, N>, dest : GraphRef<'this, 'id, E, N>) {
        let ptr = source.node as *mut GraphNode<E, N>;
        let refs = unsafe { &mut (*ptr).refs };
        refs.remove(&dest.node);
    }
}

pub struct IterRes<'this, 'guard : 'this, 'id : 'guard, E, N> {
    ptr : GraphRef<'guard, 'id, E, N>,
    node : &'this N,
    edge : &'this E
}

pub struct CursorIterator<'guard, 'id: 'guard, Iter> {    
    iter : Iter,
    _guard : &'guard Guard<'id>
}

impl <'this, 'guard : 'this, 'id : 'guard, E : 'this, N : 'this, Iter : 'this> Iterator for CursorIterator<'guard, 'id, Iter>
    where Iter : Iterator<Item = (&'this *const GraphNode<E, N>, &'this E)> {

    type Item = IterRes<'this, 'guard, 'id, E, N>;
    fn next(&mut self) -> Option<Self::Item> {
        if let Some(ptr) = self.iter.next() {
            let r = GraphRef { node : *(ptr.0), _guard : PhantomData };
            let node = *(ptr.0);
            unsafe {
                Some(IterRes { ptr : r, node : &(*node).payload, edge : ptr.1})
            }
        } else {
            None
        }
    }
}

pub struct RootIterator<'guard, 'id: 'guard, Iter> {    
    iter : Iter,
    _guard : &'guard Guard<'id>
}

impl <'this, 'guard : 'this, 'id : 'guard, E : 'this, N : 'this, Iter : 'this> Iterator for RootIterator<'guard, 'id, Iter>
    where Iter : Iterator<Item = &'this *const GraphNode<E, N>> {

    type Item = GraphRef<'guard, 'id, E, N>;
    fn next(&mut self) -> Option<Self::Item> {
        if let Some(ptr) = self.iter.next() {
            Some(GraphRef { node : *ptr, _guard : PhantomData })
        } else {
            None
        }
    }
}


/*
struct CursorMut<'this, 'anchor : 'this, 'id : 'anchor, E, N> {
    anchor : &'this mut AnchorMut<'anchor, 'id, E, N>,
    current : *mut GraphNode<E, N>
}


pub struct CursorIteratorMut<Iter> {    
    iter : Iter,
}

impl <'a, T : 'a, Iter : 'a> Iterator for CursorIteratorMut<Iter> where Iter : Iterator<Item = &'a *const GraphNode<T>> {
    type Item = (GraphRef<T>, &'a mut T);
    fn next(&mut self) -> Option<Self::Item> {
        if let Some(ptr) = self.iter.next() {
            let node = *ptr;
            let ptr = node as *mut GraphNode<T>;
            unsafe {
                Some((GraphRef { node : node, gen : self.gen }, &mut (*ptr).payload))
            }
        } else {
            None
        }
    }
}
*/


fn test<'a,'id : 'a>(a : &GraphRef<'a, 'id, i32, i32>, b : &GraphRef<'a, 'id, i32, i32>){

}

fn main() {
//    let mut graph = Graph::<i32, i32>::new();
    make_guard!(g);
    make_guard!(g2);
    let r;
    let r2;
    dbg!(size_of::<GraphRef<i32,i32>>());

    {

        let n = GraphNode::<i32, i32>::from_payload(123);
        let n2 = GraphNode::<i32, i32>::from_payload(123);
        let iter = n.refs.iter();
        let iter2 = n2.refs.iter();
        let mut iter = CursorIterator { iter : iter, _guard : &g};
        let mut iter2 = CursorIterator { iter : iter2, _guard : &g2};
        r = iter.next().unwrap().ptr;
        r2 = iter2.next().unwrap().ptr;

    }
}