use std::collections::HashMap;
use std::collections::VecDeque;

use dynamic_graph::*;

#[derive(PartialEq, Eq)]
struct BfsNode {
    key : i32,
    distance : i32
}


fn breadth_first_search(gr : &mut VecGraph<NamedNode<BfsNode, ()>>) {
    make_anchor_mut!(anchor, gr, Never);

    let root =  {
        let mut iter = anchor.iter();
        iter.next().unwrap().1
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
            let ptr = i.ptr;
            let i = i.values.that().this;
            if i.distance == -1 {
                queue.push_back(ptr);
                i.distance = dist + 1;
                println!("Touching {} distance {}", i.key, i.distance);
            }
        }
    }
}

#[test]
fn test_bfs_view() {
    let mut graph = VecGraph::<NamedNode<BfsNode, ()>>::new();
    {
        make_anchor_mut!(anchor, graph, Never);
        
        let mut vec = Vec::new();
        //Thomas Cormen, Introduction to Algorithms 2e, pic. 22.3
        for i in 0..8 {
            vec.push(anchor.spawn_detached(BfsNode { key : i, distance : -1}));
        }
        anchor.attach(vec[0]);

        let mut curs = anchor.cursor_mut(vec[0]); //option one
        for i in &[vec[1], vec[3]] {
            curs.refs.insert(*i, ());
        }        

        for i in &[vec[0], vec[2]] { //option two, will look nicer as IndexMut
            anchor.view_mut(vec[1]).refs.insert(*i, ());
        }

        anchor.view_mut(vec[2]).refs.insert(vec[1], ());

        for i in &[vec[0], vec[4], vec[5]] {
            anchor.view_mut(vec[3]).refs.insert(*i, ());
        }

        for i in &[vec[3], vec[5], vec[6]] {
            anchor.view_mut(vec[4]).refs.insert(*i, ());
        }

        for i in &[vec[3], vec[4], vec[6], vec[7]] {
            anchor.view_mut(vec[5]).refs.insert(*i, ());
        }

        for i in &[vec[4], vec[5], vec[7]] {
            anchor.view_mut(vec[6]).refs.insert(*i, ());
        }

        for i in &[vec[5], vec[6]] {
            anchor.view_mut(vec[7]).refs.insert(*i, ());
        }
    }
    breadth_first_search(&mut graph);
}

#[test]
fn test_bfs() {
    let mut graph = VecGraph::<NamedNode<BfsNode, ()>>::new();
    {
        make_anchor_mut!(anchor, graph, Never);
        
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

        let c = anchor.cursor(vec[0]);
    }
    breadth_first_search(&mut graph);
}

type BFRef<'id> = GraphPtr<'id, NamedNode<usize, usize>>;

fn bellman_ford<'id>(graph : &AnchorMut<'id, 'id, VecGraph<NamedNode<usize, usize>>>, count : usize,
                         source : BFRef<'id>) -> HashMap::<BFRef<'id>, BFRef<'id>>
{
    let mut distance = HashMap::new();
    let mut path = HashMap::new();//(to;from)

    let mut cursor = graph.cursor(source);
    for i in cursor.iter() {
        distance.insert(i.ptr, *i.values.this().edge);
        path.insert(i.ptr, source);
    }
    distance.insert(source, 0);

    for _ in 0..count - 1 {
        let nodes : Vec<_> = distance.keys().map(|x| {*x}).collect();
        for i in nodes {
            cursor.jump(i);
            for j in cursor.iter() {
                let edge = j.values.this().edge;
                let j = j.ptr;
                if !distance.contains_key(&j) ||
                    distance[&j] > distance[&i] + edge {
                    path.insert(j, i);
                    distance.insert(j, distance[&i] + edge);
                }
            }
        }
    }
    path
}


fn print_bf_path<'id, 'a>(graph : &AnchorMut<'a, 'id, VecGraph<NamedNode<usize, usize>>>,
                path : &HashMap::<BFRef<'id>, BFRef<'id>>,
                source : BFRef<'id>, target : BFRef<'id>) -> usize {
    let mut cursor = graph.cursor(target);
    let mut full_len = 0;
    if path.contains_key(&target) {
        while !cursor.is_at(source) {
            let cur = cursor.at();
            let prev = path[&cur];

            let cur_key = *cursor.get();
            let prev_key = graph[prev];

            cursor.jump(prev);

            let len = cursor.get_edge(cur).unwrap().this().edge;
            full_len += len;
            println!("{} to {}, len {}", prev_key, cur_key, len);
        }
    }
    full_len
}

#[test]
fn shortest_path_test() {
    let mut graph = VecGraph::new();
    make_anchor_mut!(anchor, graph, Never);
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
    assert!(print_bf_path(&anchor, &path, source, n1) == 8);
    assert!(print_bf_path(&anchor, &path, source, n2) == 5);
    assert!(print_bf_path(&anchor, &path, source, n3) == 9);
    assert!(print_bf_path(&anchor, &path, source, n4) == 7);
}

