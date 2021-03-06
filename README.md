# dynamic_graph
This crate in an experimental phase is subject to radical changes. 

 There are two key concepts to understanding of this library. The first such concept is called Anchor.  It is pretty much impossible to create robust graph-related software without reliance upon some form of garbage collection. It is not an issue for a language with a huge runtime environment like Python or Java since they govern every memory allocation and may stop execution to perform garbage collection and memory relocation at any moment. This is difficult to implement in Rust and even harder to integrate with the rest of the language infrastructure. 

The first part of the solution is to treat graphs as another collection just like LinkedList. However, graph manipulation requires the ability to juggle references LinkedList does not provide in a safe manner (or at all). The second part of the solution is a LockGuard-like structure which allows user to look at and modify the contents of the graph, add new entries, but not drop existing entries.  This structure is Anchor[Mut].
 Being the only point of access to graph data, Anchor allows mutating graph entries without the use of interior mutability.  The destructor of the anchor makes for a perfect deterministic 'safe point' to perform garbage collection if required.  The absence of highly intrusive runtime hooks like Boehm GC means it is trivial to use existing collections and classes with this crate and opt-into manual memory management for critical cases. And type-level voodoo provides all of this without any runtime cost!

The second concept to understand is a Cursor. This object can be seen as an advanced iterator that may move to an arbitrary graph entry and modify node data, add or remove edges and iterate over neighboring nodes.

TODO LIST: 
- [ ] Smarter cleanup strategies
- [ ] More types of nodes
- [ ] Better samples and docs
- [ ] User defined types
- [ ] Get rid of lazy shortcuts