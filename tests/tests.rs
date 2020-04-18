use std::collections::HashMap;
use std::collections::VecDeque;
use std::cmp::*;
use dynamic_graph::*;

use dynamic_graph::CleanupStrategy::*;

struct BfsNode {
    key : i32,
    distance : i32
}

fn breadth_first_search(graph : &mut VecGraph<NamedNode<BfsNode, ()>>) {
    anchor_mut!(graph, AlwaysPrecise);

    let root =  graph.root()[0];
    let mut cursor = graph.cursor_mut(root);

    cursor.data.distance = 0;
    let mut queue = VecDeque::new();
    queue.push_back(root);

    while !queue.is_empty() {
        let q = queue.pop_front().unwrap();
        cursor.jump(q);
        println!("Visiting {}", cursor.data.key);

        let dist = cursor.data.distance;

        for i in cursor.edges_mut() {
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
fn test_bfs() {
    let mut graph = VecGraph::new();
    {
        anchor_mut!(graph, Never);
        
        let mut vec = Vec::new();
        //Thomas Cormen, Introduction to Algorithms 2e, pic. 22.3
        for i in 0..8 {
            vec.push(graph.spawn(BfsNode { key : i, distance : -1}));
        }
        graph.root_mut().push(vec[0]);

        for i in &[vec[1], vec[3]] {
            graph[vec[0]].refs.insert(*i, ());
        }        

        for i in &[vec[0], vec[2]] {
            graph[vec[1]].refs.insert(*i, ());
        }

        graph[vec[2]].refs.insert(vec[1], ());

        for i in &[vec[0], vec[4], vec[5]] {
            graph[vec[3]].refs.insert(*i, ());
        }

        for i in &[vec[3], vec[5], vec[6]] {
            graph[vec[4]].refs.insert(*i, ());
        }

        for i in &[vec[3], vec[4], vec[6], vec[7]] {
            graph[vec[5]].refs.insert(*i, ());
        }

        for i in &[vec[4], vec[5], vec[7]] {
            graph[vec[6]].refs.insert(*i, ());
        }

        for i in &[vec[5], vec[6]] {
            graph[vec[7]].refs.insert(*i, ());
        }
    }
    breadth_first_search(&mut graph);
}

type BFNode = NamedNode<usize, usize>;
type BFRef<'id> = GraphPtr<'id, BFNode>;

fn bellman_ford<'a>(graph : &AnchorMut<'a, 'a, VecGraph<BFNode>>,
                    count : usize, source : BFRef<'a>) -> HashMap::<BFRef<'a>, BFRef<'a>>
{
    let mut distance = HashMap::new();//from source node
    let mut path = HashMap::new();//(to;from)

    if count == 0 {
        return path;
    }

    let mut cursor = graph.cursor(source);
    for i in cursor.edges() {
        distance.insert(i.ptr, *i.values.edge());
        path.insert(i.ptr, source);
    }
    distance.insert(source, 0);

    for _ in 0..count - 1 {
        let nodes : Vec<_> = distance.keys().map(|x| {*x}).collect();
        for i in nodes {
            cursor.jump(i);
            for j in cursor.edges() {
                let edge = j.values.edge();
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


fn print_bf_path<'a>(graph : &AnchorMut<'a, 'a, VecGraph<BFNode>>,
                path : &HashMap::<BFRef<'a>, BFRef<'a>>,
                source : BFRef<'a>, target : BFRef<'a>) -> usize {
    let mut cursor = graph.cursor(target);
    let mut full_len = 0;
    if path.contains_key(&target) {
        while !cursor.is_at(source) {
            let cur = cursor.at();
            let prev = path[&cur];

            let cur_key = cursor.data;
            let prev_key = graph[prev].data;

            cursor.jump(prev);

            let len = cursor.get_edge(cur).edge().unwrap();
            full_len += len;
            println!("{} to {}, len {}", prev_key, cur_key, len);
        }
    }
    full_len
}

#[test]
fn shortest_path_test() {
    let mut graph = VecGraph::new(); 
    {
        anchor_mut!(graph, Never);
        //Thomas Cormen, Introduction to Algorithms 2e, pic. 24.6

        let source = graph.spawn_attached(0);

        let n1 = graph.spawn(1);
        let n2 = graph.spawn(2);

        let n3 = graph.spawn(3);
        let n4 = graph.spawn(4);

        let refs = &mut graph[source].refs;
        refs.insert(n1, 10);
        refs.insert(n2, 5);

        let refs = &mut graph[n1].refs;
        refs.insert(n2, 2);
        refs.insert(n3, 1);
        
        let refs = &mut graph[n2].refs;

        refs.insert(n1, 3);
        refs.insert(n4, 2);
        refs.insert(n3, 9);

        let refs = &mut graph[n4].refs;
        refs.insert(n3, 6);
        refs.insert(source, 7);

        graph[n3].refs.insert(n4, 4);

        let path = bellman_ford(&graph, 5, source);
        assert!(print_bf_path(&graph, &path, source, n1) == 8);
        assert!(print_bf_path(&graph, &path, source, n2) == 5);
        assert!(print_bf_path(&graph, &path, source, n3) == 9);
        assert!(print_bf_path(&graph, &path, source, n4) == 7);
    }
}

#[test]
fn test_kill_smoke() {
    let mut graph = VecGraph::<NamedNode<_, ()>>::new();
    anchor_mut!(graph, AlwaysPrecise);

    let source = graph.spawn(0);
    unsafe {
        graph.kill(source);
    }
}


struct FlowEdge {
    capacity : i32,
    flow : i32,
}

type FlowNode = NamedNode<(), FlowEdge>;
type FlowRef<'id> = GraphPtr<'id, FlowNode>;

fn find_path<'id>(graph : &AnchorMut<'id, 'id, VecGraph<FlowNode>>)
               -> Option<HashMap<FlowRef<'id>, (FlowRef<'id>, i32)>>
{
    let mut path = HashMap::new();
    let mut queue = VecDeque::new();

    let root = graph.root();

    let source = root[0];
    let sink = root[1];

    path.insert(source, (source, 0));
    queue.push_back(source);
    
    while !queue.is_empty() {
        let q = queue.pop_front().unwrap();

        for i in graph.edges(q) {
            let ptr = i.ptr;
            let i = i.values.edge();
            if !path.contains_key(&ptr) && i.capacity - i.flow > 0 {
                path.insert(ptr, (q, i.capacity - i.flow));
                queue.push_back(ptr);
                if ptr == sink {
                    return Some(path);
                }
            }
        }
    }
    None
}

fn edmonds_karp(graph : &mut VecGraph<FlowNode>) -> i32 {
    anchor_mut!(graph, Never);

    let root = graph.root();

    let source = root[0];
    let sink = root[1];

    while let Some(path) = find_path(&graph) {
        let last_step = &path[&sink];

        let mut current = last_step.0;
        let mut max_cut = last_step.1;

        while current != source {
            let cur_step = path[&current];
            max_cut = min(max_cut, cur_step.1);
            current = cur_step.0;
        }

        let mut cursor = graph.cursor_mut(sink);
        while !cursor.is_at(source) {
            let at = cursor.at();
            let cur_step = path[&at];

            cursor.refs.entry(cur_step.0).and_modify(|x| x.flow -= max_cut);

            cursor.jump(cur_step.0);
            cursor.refs.entry(at).and_modify(|x| x.flow += max_cut);
        }
    }
    let flow = graph.edges(sink).map(|x| -x.values.edge().flow).sum();

    flow
}

#[test]
fn test_max_flow() {
    let mut graph = VecGraph::new();
    {
        //Thomas Cormen, Introduction to Algorithms 2e, pic. 26.5
        anchor_mut!(graph, AlwaysPrecise);
        let source = graph.spawn_attached(());
        let sink   = graph.spawn_attached(());

        let v1 = graph.spawn(());
        let v2 = graph.spawn(());
        let v3 = graph.spawn(());
        let v4 = graph.spawn(());

        let f = |capacity| FlowEdge { capacity, flow : 0 };
        graph[source].refs.insert(v1,     f(16));
        graph[v1]    .refs.insert(source, f(0 ));

        graph[source].refs.insert(v2,     f(13));
        graph[v2]    .refs.insert(source, f(0 ));

        graph[v2].refs.insert(v1, f(4 ));
        graph[v1].refs.insert(v2, f(10));

        graph[v1].refs.insert(v3, f(12));
        graph[v3].refs.insert(v1, f(0 ));

        graph[v3].refs.insert(v2, f(9));
        graph[v2].refs.insert(v3, f(0));

        graph[v4].refs.insert(v3, f(7));
        graph[v3].refs.insert(v4, f(0));

        graph[v2].refs.insert(v4, f(14));
        graph[v4].refs.insert(v2, f(0 ));

        graph[v3]  .refs.insert(sink, f(20));
        graph[sink].refs.insert(v3,   f(0 ));

        graph[v4]  .refs.insert(sink, f(4));
        graph[sink].refs.insert(v4,   f(0));
    }
    assert_eq!(edmonds_karp(&mut graph), 23);
}