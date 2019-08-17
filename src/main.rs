use std::collections::{VecDeque, HashMap};

mod dynamic_graph;
use crate::dynamic_graph::{Graph, AnchorMut, GraphRef};


#[derive(Debug)]
struct BfsNode {
    key : i32,
    distance : i32
}

fn breadth_first_search(gr : &mut Graph<BfsNode>) {
    let mut anchor = gr.anchor_mut();//Guarantees we can safely store references to vertices in external collections while this is alive
    let root =  {
        let mut iter = anchor.iter();
        iter.next().unwrap().0 //0 is a reference to the vertex object, 1 is reference to vertex data. Using tuple was a dumber idea than expected 
    };
    
    let mut cursor = anchor.cursor_mut(root);
    //Cursor is a mix between a pointer and an iterator. Unlike iterator, it can freely jump between elements of a graph,
    //but unlike pointer it's only allowed to work with elements of the parent collection.
    cursor.get_mut().distance = 0;
    let mut queue : VecDeque<GraphRef<BfsNode>> = VecDeque::new();
    queue.push_back(root);

    while !queue.is_empty() {
        let q = queue.pop_front().unwrap();
        println!("Visiting {}", cursor[q].key);
        cursor.jump(q);
        let dist = cursor.get().distance;

        for i in cursor.iter_mut() {
            if i.1.distance == -1 {
                queue.push_back(i.0);
                i.1.distance = dist + 1;
                println!("Touching {} distance {}", i.1.key, i.1.distance);
            }
        }
    }
}



fn test_bfs(){
    let mut graph = Graph::<BfsNode>::new();
    {
        let mut anchor = graph.anchor_mut();
        let root = anchor.add(BfsNode { key : 0, distance : -1});
        
        let mut cursor = anchor.cursor_mut(root);
        //Thomas Cormen, Introduction to Algorithms 2e, pic. 22.3

        let son = cursor.add_sym(BfsNode { key : 1, distance : -1});
        
        {
            cursor.jump(son);
            cursor.add_sym(BfsNode { key : 2, distance : -1});
        }
        cursor.jump(root);
        let son = cursor.add_sym(BfsNode { key : 3, distance : -1});
        {
            cursor.jump(son);
            let g_son1 = cursor.add_sym(BfsNode { key : 4, distance : -1});
            let g_son2 = cursor.add_sym(BfsNode { key : 5, distance : -1});
            
            cursor.jump(g_son2);

            cursor.attach_sym(g_son1);

            let gg_son1 = cursor.add_sym(BfsNode { key : 6, distance : -1});
            let gg_son2 = cursor.add_sym(BfsNode { key : 7, distance : -1});

            cursor.jump(gg_son1);
            cursor.attach_sym(gg_son2);
        }
     }
    breadth_first_search(&mut graph);
}


#[derive(PartialEq, Eq)]
struct BFNode {
    key : usize,
    len : HashMap<usize, usize>
}

impl BFNode {
    fn new(key : usize) -> BFNode {
        BFNode { key : key, len : HashMap::new() }
    }
}

fn bellman_ford(graph : &AnchorMut<BFNode>, count : usize, source : GraphRef<BFNode>) -> 
                HashMap::<GraphRef<BFNode>, GraphRef<BFNode>>
{
    let mut dist = HashMap::new();
    let mut path = HashMap::<GraphRef<_>, GraphRef<_>>::new();//(to;from)

    let mut cursor = graph.cursor(source);
    for i in cursor.iter() {
        dist.insert(i.0, cursor.get().len[&i.1.key]);
        path.insert(i.0, source);
    }
    dist.insert(source, 0);

    for _ in 0..count - 1 {
        let nodes : Vec<_> = dist.keys().map(|x| {*x}).collect();
        for i in nodes {
            cursor.jump(i);
            for j in cursor.iter() {
                let key = j.1.key;
                if !dist.contains_key(&j.0) ||
                    dist[&j.0] > dist[&i] + cursor[i].len[&key] {
                    path.insert(j.0, i);
                    dist.insert(j.0, dist[&i] + cursor[i].len[&key]);
                }
            }
        }
    }
    path
}
fn print_bf_path(graph : &AnchorMut<BFNode>, path : &HashMap::<GraphRef<BFNode>, GraphRef<BFNode>>,
                 source : GraphRef<BFNode>, target : GraphRef<BFNode>) {
    let mut cursor = graph.cursor(target);
    if path.contains_key(&target) {
        let mut whole = 0;
        while !cursor.is_at(source) {
            let cur = cursor.at();
            let prev = path[&cur];

            let cur_key = cursor.get().key;
            let prev_key = cursor[prev].key;
            let len = cursor[prev].len[&cur_key];
            whole += len;
            println!("{} to {}, len {}", prev_key, cur_key, len);

            cursor.jump(prev);
        }
        println!("Length {}", whole);
    }
    println!("_________");
}

fn shortest_path_test(){
    let mut graph = Graph::new();
    let mut anchor = graph.anchor_mut();
    //Thomas Cormen, Introduction to Algorithms 2e, pic. 24.6

    let source = anchor.add(BFNode::new(0));

    let n1 = anchor.add(BFNode::new(1));
    let n2 = anchor.add(BFNode::new(2));

    let n3 = anchor.add(BFNode::new(3));
    let n4 = anchor.add(BFNode::new(4));

    let mut cursor = anchor.cursor_mut(source);
    cursor.attach(n1);
    cursor.attach(n2);
    let r = cursor.get_mut();
    r.len.insert(1, 10);
    r.len.insert(2, 5);

    cursor.jump(n1);
    cursor.attach_sym(n2);
    cursor.attach(n3);
    let r = cursor.get_mut();
    r.len.insert(2, 2);
    r.len.insert(3, 1);

    cursor.jump(n2);
    cursor.attach(n3);
    cursor.attach(n4);

    let r = cursor.get_mut();
    r.len.insert(1, 3);
    r.len.insert(4, 2);
    r.len.insert(3, 9);

    cursor.jump(n4);
    cursor.attach_sym(n3);
    let r = cursor.get_mut();
    r.len.insert(3, 6);
    r.len.insert(0, 7);

    cursor.jump(n3);
    cursor.get_mut().len.insert(4, 4);

    let path = bellman_ford(&anchor, 5, source);
    print_bf_path(&anchor, &path, source, n1);
    print_bf_path(&anchor, &path, source, n2);
    print_bf_path(&anchor, &path, source, n3);
    print_bf_path(&anchor, &path, source, n4);
}

fn main(){

    test_bfs();
    println!("");
    shortest_path_test();
}