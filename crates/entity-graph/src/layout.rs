//! Where the boxes go: layers left to right, and which edges run backwards.
//!
//! # Why there is no layout engine here
//!
//! Calling graphviz would mean a drawing this repository cannot reproduce — a different `dot`
//! version on a different machine moves the boxes, and a picture that changes without the
//! definition changing is one nobody can review in a pull request. It would also put a process
//! spawn behind a verb whose whole subject is data. So the layering is four hundred lines of
//! integer arithmetic done here, and the same definition gives the same coordinates on every
//! machine, for ever.
//!
//! # The algorithm, and its one interesting part
//!
//! Longest-path layering: a node sits one layer to the right of the furthest node that reaches it.
//! That needs an acyclic graph and a lifecycle is rarely acyclic — `proposed → draft` runs back up
//! the ladder, and so does every *request changes* edge anybody writes.
//!
//! So back edges are classified **first**, by depth-first search from the entry: an edge that
//! arrives at a node still open on the stack is a back edge, and layering ignores it. A renderer
//! gets told which they are, because an edge that runs right-to-left has to be drawn differently or
//! it lands under the boxes it passes.

use std::collections::{BTreeMap, BTreeSet};

use crate::graph::{Emphasis, Graph};

/// Where everything goes.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Layout {
    /// Node indices, by layer, left to right. Within a layer, in the graph's own node order.
    pub layers: Vec<Vec<usize>>,
    /// Indices of edges that run backwards, against the layering.
    pub back_edges: BTreeSet<usize>,
}

impl Layout {
    /// Lays a graph out.
    ///
    /// Total: a graph with no nodes lays out to nothing, a node nothing reaches still gets a layer,
    /// and a cycle with no entry at all still terminates — none of which is obvious, so each has a
    /// test.
    #[must_use]
    pub fn of(graph: &Graph) -> Self {
        if graph.nodes.is_empty() {
            return Self::default();
        }
        let index: BTreeMap<&str, usize> = graph
            .nodes
            .iter()
            .enumerate()
            .map(|(at, node)| (node.id.as_str(), at))
            .collect();

        let mut out: Vec<Vec<usize>> = vec![Vec::new(); graph.nodes.len()];
        let mut resolved = Vec::with_capacity(graph.edges.len());
        for edge in &graph.edges {
            match (index.get(edge.from.as_str()), index.get(edge.to.as_str())) {
                // An edge naming a node the graph does not hold is dropped rather than refused:
                // this type draws what it is given, and refusing is the caller's job.
                (Some(&from), Some(&to)) => {
                    out[from].push(to);
                    resolved.push(Some((from, to)));
                }
                _ => resolved.push(None),
            }
        }

        let back = Self::back_edges(graph, &out, &resolved);
        let layers = Self::layer(graph, &resolved, &back);
        Self {
            layers,
            back_edges: back,
        }
    }

    /// Depth-first from every entry, then from anything still unvisited, marking edges that arrive
    /// at an open node.
    ///
    /// Iterative rather than recursive: a ladder is small, but a definition is data an adopter
    /// writes, and a hand-written thousand-state machine should draw rather than overflow a stack.
    fn back_edges(
        graph: &Graph,
        out: &[Vec<usize>],
        resolved: &[Option<(usize, usize)>],
    ) -> BTreeSet<usize> {
        #[derive(Clone, Copy, PartialEq)]
        enum Mark {
            Fresh,
            Open,
            Done,
        }
        let mut mark = vec![Mark::Fresh; graph.nodes.len()];
        let mut back = BTreeSet::new();

        for start in Self::starts(graph, resolved) {
            if mark[start] != Mark::Fresh {
                continue;
            }
            // (node, how many of its out-edges have been taken)
            let mut stack = vec![(start, 0usize)];
            mark[start] = Mark::Open;
            while let Some((node, taken)) = stack.pop() {
                if taken < out[node].len() {
                    let next = out[node][taken];
                    stack.push((node, taken + 1));
                    match mark[next] {
                        Mark::Fresh => {
                            mark[next] = Mark::Open;
                            stack.push((next, 0));
                        }
                        // Still on the stack: this edge closes a loop.
                        Mark::Open => {
                            for (at, edge) in resolved.iter().enumerate() {
                                if *edge == Some((node, next)) {
                                    back.insert(at);
                                }
                            }
                        }
                        Mark::Done => {}
                    }
                } else {
                    mark[node] = Mark::Done;
                }
            }
        }
        back
    }

    /// Every node worth starting a search from, best first: declared entries, then nodes nothing
    /// arrives at, then everything else so a graph that is one closed loop still gets visited.
    fn starts(graph: &Graph, resolved: &[Option<(usize, usize)>]) -> Vec<usize> {
        let mut arrived_at = vec![false; graph.nodes.len()];
        for edge in resolved.iter().flatten() {
            arrived_at[edge.1] = true;
        }
        let entries: Vec<usize> = graph
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, node)| node.emphasis == Emphasis::Entry)
            .map(|(at, _)| at)
            .collect();
        let roots: Vec<usize> = (0..graph.nodes.len())
            .filter(|at| !arrived_at[*at])
            .collect();
        let mut order = entries;
        order.extend(roots);
        order.extend(0..graph.nodes.len());
        order
    }

    /// Longest path from the left, over forward edges only.
    ///
    /// Relaxed until nothing moves, which terminates because the forward edges form a DAG — every
    /// cycle has at least one edge classified back, by construction of the search above.
    fn layer(
        graph: &Graph,
        resolved: &[Option<(usize, usize)>],
        back: &BTreeSet<usize>,
    ) -> Vec<Vec<usize>> {
        let forward: Vec<(usize, usize)> = resolved
            .iter()
            .enumerate()
            .filter(|(at, _)| !back.contains(at))
            .filter_map(|(_, edge)| *edge)
            .collect();

        let mut depth = vec![0usize; graph.nodes.len()];
        // At most one relaxation per node per pass, and a longest path is at most n - 1 edges.
        for _ in 0..graph.nodes.len() {
            let mut moved = false;
            for &(from, to) in &forward {
                if depth[to] < depth[from] + 1 {
                    depth[to] = depth[from] + 1;
                    moved = true;
                }
            }
            if !moved {
                break;
            }
        }
        let width = depth.iter().copied().max().unwrap_or(0) + 1;
        let mut layers = vec![Vec::new(); width];
        for (at, &layer) in depth.iter().enumerate() {
            layers[layer].push(at);
        }
        layers
    }

    /// Which layer a node is in, or `None` when the graph does not hold it.
    #[must_use]
    pub fn layer_of(&self, node: usize) -> Option<usize> {
        self.layers.iter().position(|layer| layer.contains(&node))
    }

    /// How many layers there are.
    #[must_use]
    pub fn width(&self) -> usize {
        self.layers.len()
    }

    /// The most nodes in any one layer.
    #[must_use]
    pub fn height(&self) -> usize {
        self.layers.iter().map(Vec::len).max().unwrap_or(0)
    }
}
