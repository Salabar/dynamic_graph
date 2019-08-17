use std::sync::atomic::AtomicU64;
use std::collections::HashSet;
use std::collections::hash_set::Iter;
use core::sync::atomic::Ordering;
use core::ops::{IndexMut, Index};
use std::mem::transmute;
use std::hash::{Hash, Hasher};
static mut ANCHOR_COUNTER : AtomicU64 = AtomicU64::new(0);

#[derive(PartialEq, Eq)]
pub struct GraphRef<T> {
    node : *const GraphNode<T>,
    gen : u64
}

impl <T> Hash for GraphRef<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        let ptr : *const i32 = unsafe {
            transmute(self.node) 
        };
        ptr.hash(state);
    }
}


impl <T> Clone for GraphRef<T> {
    fn clone(&self) -> GraphRef<T> {
        GraphRef { node : self.node, gen : self.gen }
    }
}

impl <T> Copy for GraphRef<T> {}

pub struct GraphNode<T> {
    refs : HashSet<*const GraphNode<T>>,
    payload : T
}

impl <T> GraphNode<T> {
    fn from_payload(data : T) -> GraphNode<T> {
        GraphNode { refs : HashSet::new(), payload : data }
    }
}

pub struct Graph<T> {
    root: HashSet<*const GraphNode<T>>,
}

pub struct AnchorMut<'graph, T> {
    parent: &'graph mut Graph<T>,
    gc_required: bool,
    gen: u64
}

impl <T> Graph<T> {
    pub fn new() -> Graph<T> {
        Graph { root : HashSet::new() }
    }
}

impl <'graph, T> Graph<T> {
    pub fn anchor_mut(&'graph mut self) -> AnchorMut<'graph, T> {
        let gen = unsafe {
            ANCHOR_COUNTER.fetch_add(1, Ordering::Relaxed)
            //This can technically overflow and break some checks, but I don't want to sacrifice usabiliy for such a degenerate case
            //even if my proposal won't meet any enthusiasm
        };
        AnchorMut::<T> { parent : self, gc_required : false, gen : gen }
    }
}

impl <T> Graph<T> { 
    pub fn gc(&mut self) {
        println!("Pretend I do garbage collection here");
    }
}


pub struct CursorMut<'anchor, 'graph : 'anchor, T> {
    anchor: &'anchor mut AnchorMut<'graph, T>,
    gen : u64,
    current : *mut GraphNode<T>,
}


impl <'cursor, 'anchor : 'cursor, 'graph : 'anchor, T> AnchorMut<'graph, T> {
    pub fn cursor_mut(&'anchor mut self, target : GraphRef<T>) -> CursorMut<'cursor, 'graph, T> {
        self.check_parent(target);
        let gen = self.gen;
        let ptr = target.node as *mut GraphNode<T>;
        CursorMut { anchor : self, current : ptr, gen : gen }
    }


    pub fn cursor(&'anchor self, target : GraphRef<T>) -> Cursor<'cursor, 'graph, T> {
        self.check_parent(target);
        let gen = self.gen;
        let ptr = target.node;
        Cursor { anchor : self, current : ptr, gen : gen }
    }



    pub fn iter(&'anchor self) -> impl Iterator<Item = (GraphRef<T>, &'anchor T)> {//CursorIterator<Iter<'cursor, *const GraphNode<T>>> {
        CursorIterator { iter : self.parent.root.iter(), gen : self.gen }
    }

    pub fn iter_mut(&'anchor mut self) -> impl Iterator<Item = (GraphRef<T>, &'anchor mut T)> {// CursorIteratorMut<Iter<'cursor, *const GraphNode<T>>> {
        CursorIteratorMut { iter : self.parent.root.iter(), gen : self.gen }
    }

    fn check_parent(&self, target : GraphRef<T>){
        if self.gen != target.gen {
            panic!("Using reference of another anchor");
        }
    }

    pub fn attach(&mut self, target : GraphRef<T>) {
        self.check_parent(target);
        self.parent.root.insert(target.node);
    }

    pub fn add(&mut self, payload : T) -> GraphRef<T> {
        let node = Box::new(GraphNode::from_payload(payload));
        let res = GraphRef {gen : self.gen, node : Box::into_raw(node)};
        self.attach(res);
        res
    }
}

impl <'a, T> Drop for AnchorMut<'a, T> {
    fn drop(&mut self) {
        if self.gc_required {
            println!("This is the part where I'm supposed to collect garbage, but I don't");
        }
    }
}


pub struct CursorIterator<Iter> {    
    iter : Iter,
    gen : u64,
}

pub struct CursorIteratorMut<Iter> {    
    iter : Iter,
    gen : u64,
}

impl <'a, T : 'a, Iter : 'a> Iterator for CursorIterator<Iter> where Iter : Iterator<Item = &'a *const GraphNode<T>> {
    type Item = (GraphRef<T>, &'a T);
    fn next(&mut self) -> Option<Self::Item> {
        if let Some(ptr) = self.iter.next() {
            let node = *ptr;
            unsafe {
                Some((GraphRef { node : node, gen : self.gen }, &(*node).payload))
           }
        } else {
            None
        }
    }
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

impl <'anchor, 'graph : 'anchor, T> CursorMut<'anchor, 'graph, T> {
    pub fn at(&self) -> GraphRef<T> {
        GraphRef { node : self.current, gen : self.gen }
    }

    pub fn is_at(&self, target : GraphRef<T>) -> bool {
        self.current == target.node as *mut GraphNode<T>
    }

    pub fn get(&self) -> &T {
        unsafe {
            &(*self.current).payload
        }
    }
    pub fn get_mut(&mut self) -> &mut T {
        unsafe {
            &mut (*self.current).payload
        }
    }
        
    pub fn iter(&self) -> impl Iterator<Item = (GraphRef<T>, &T)> {
        let node = unsafe {
            &*self.current
        };
        CursorIterator { iter : node.refs.iter(), gen : self.gen }
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = (GraphRef<T>, &mut T)> {
        let node = unsafe {
            &*self.current
        };
        CursorIteratorMut { iter : node.refs.iter(), gen : self.gen }
    }

    fn check_parent(&self, target : GraphRef<T>){
        if self.gen != target.gen {
            panic!("Using reference of another anchor");
        }
    }

    pub fn attach(&mut self, target : GraphRef<T>) {
        self.check_parent(target);
        let node;
        unsafe {
            node = &mut *(self.current);
        }
        node.refs.insert(target.node);
    }

    pub fn attach_sym(&mut self, target : GraphRef<T>) {
        self.attach(target);
        let ptr = target.node as *mut GraphNode<T>;
        if self.current != ptr {
            let that;
            unsafe {
                that = &mut *ptr;
            }
            that.refs.insert(self.current);
        }
    }

    pub fn add(&mut self, payload : T) -> GraphRef<T> {
        let node = Box::new(GraphNode::from_payload(payload));
        let res = GraphRef {gen : self.gen, node : Box::into_raw(node)};
        self.attach(res);
        res
    }

    pub fn add_sym(&mut self, payload : T) -> GraphRef<T> {
        let node = Box::new(GraphNode::from_payload(payload));
        let res = GraphRef {gen : self.gen, node : Box::into_raw(node)};
        self.attach_sym(res);
        res
    }


    pub fn detach(&mut self, target : GraphRef<T>) {
        self.check_parent(target);
        let node;
        unsafe {
            node = &mut *(self.current);
        }
        node.refs.remove(&target.node);
    }
    pub fn detach_sym(&mut self, target : GraphRef<T>) {
        self.detach(target);
        let ptr = target.node as *mut GraphNode<T>;
        if self.current != ptr {
            let that;
            unsafe {
                that = &mut *ptr;
            }
            let this = self.current as *const GraphNode<T>;
            that.refs.remove(&this);
        }
    }

    pub fn jump(&mut self, target : GraphRef<T>) {
        self.check_parent(target);
        self.current = target.node as *mut GraphNode<T>;
    }

    pub fn bridge(&mut self, target : GraphRef<T>) -> Option<(&mut T, &mut T)> {
        self.check_parent(target);
        let other = target.node as *mut GraphNode<T>;
        if self.current != other {
            unsafe {
                let res =(&mut (*self.current).payload, &mut (*other).payload); 
                Some(res)
            }
        } else {
            None
        }
    }
}

impl <'anchor, 'graph : 'anchor, T> Index<GraphRef<T>> for CursorMut<'anchor, 'graph, T> {
    type Output = T;
    fn index(&self, target : GraphRef<T>) -> &Self::Output {
        self.check_parent(target);

        let that;
        unsafe {
            that = &*target.node;
        }
        &that.payload
    }
}

impl <'anchor, 'graph : 'anchor, T> IndexMut<GraphRef<T>> for CursorMut<'anchor, 'graph, T> {
    fn index_mut(&mut self, target : GraphRef<T>) -> &mut Self::Output {
    self.check_parent(target);
    let ptr = target.node as *mut GraphNode<T>;
    let that = unsafe {
            &mut *ptr
        };
        &mut that.payload
    }
}

#[derive(Clone, Copy)]
pub struct Cursor<'anchor, 'graph : 'anchor, T> {
    anchor: &'anchor AnchorMut<'graph, T>,
    gen : u64,
    current : *const GraphNode<T>,
}

impl <'anchor, 'graph : 'anchor, T> Cursor<'anchor, 'graph, T> {
    pub fn at(&self) -> GraphRef<T> {
        GraphRef { node : self.current, gen : self.gen }
    }

    pub fn is_at(&self, target : GraphRef<T>) -> bool {
        self.check_parent(target);
        self.current == target.node
    }
    pub fn get(&self) -> &T {
        unsafe {
            &(*self.current).payload
        }
    }

    pub fn iter(&self) -> CursorIterator<Iter<'anchor, *const GraphNode<T>>> {
        let node = unsafe {
            &*self.current
        };
        CursorIterator { iter : node.refs.iter(), gen : self.gen }
    }

    fn check_parent(&self, target : GraphRef<T>){
        if self.gen != target.gen {
            panic!("Using reference of another anchor");
        }
    }

    pub fn jump(&mut self, target : GraphRef<T>) {
        self.check_parent(target);
        self.current = target.node;
    }
}

impl <'anchor, 'graph : 'anchor, T> Index<GraphRef<T>> for Cursor<'anchor, 'graph, T> {
    type Output = T;
    fn index(&self, target : GraphRef<T>) -> &Self::Output {
        self.check_parent(target);

        let that;
        unsafe {
            that = &*target.node;
        }
        &that.payload
    }
}
