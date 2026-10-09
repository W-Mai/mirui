use alloc::vec::Vec;

#[derive(Default)]
pub(crate) struct OutputGraph {
    nodes: Vec<Node>,
    edges: Vec<(usize, usize)>,
    order: Vec<usize>,
}

#[derive(Default)]
struct Node {
    indegree: usize,
    remaining: usize,
}

impl OutputGraph {
    pub(crate) fn clear(&mut self) {
        self.nodes.clear();
        self.edges.clear();
        self.order.clear();
    }

    pub(crate) fn add_node(&mut self) -> usize {
        let index = self.nodes.len();
        self.nodes.push(Node::default());
        self.order.clear();
        // Prepare queue capacity while building, never while resolving.
        self.order.reserve(self.nodes.len());
        index
    }

    pub(crate) fn add_dependency(&mut self, producer: usize, consumer: usize) {
        assert!(producer < self.nodes.len(), "unknown output producer");
        assert!(consumer < self.nodes.len(), "unknown output consumer");
        let edge = (producer, consumer);
        if let Err(index) = self.edges.binary_search(&edge) {
            self.edges.insert(index, edge);
            self.nodes[consumer].indegree += 1;
            self.order.clear();
        }
    }

    /// Resolve a deterministic producer-first order without allocating. Cycles and their dependent successors remain excluded.
    pub(crate) fn resolve(&mut self) -> &[usize] {
        self.order.clear();
        for (index, node) in self.nodes.iter_mut().enumerate() {
            node.remaining = node.indegree;
            if node.remaining == 0 {
                self.order.push(index);
            }
        }

        // The retained order also serves as the ready queue. Sorted edges make
        // each producer's successor order independent of edge insertion order.
        let mut cursor = 0;
        while cursor < self.order.len() {
            let current = self.order[cursor];
            cursor += 1;
            let start = self
                .edges
                .partition_point(|&(producer, _)| producer < current);
            for &(producer, consumer) in &self.edges[start..] {
                if producer != current {
                    break;
                }
                let node = &mut self.nodes[consumer];
                node.remaining -= 1;
                if node.remaining == 0 {
                    self.order.push(consumer);
                }
            }
        }
        &self.order
    }

    pub(crate) fn order(&self) -> &[usize] {
        &self.order
    }

    /// Query the last resolution. A blocked node is either cyclic or depends on a cycle; it is not necessarily a cycle member.
    pub(crate) fn is_blocked(&self, index: usize) -> bool {
        self.nodes[index].remaining != 0
    }

    pub(crate) fn dependencies(&self, consumer: usize) -> impl Iterator<Item = usize> + '_ {
        assert!(consumer < self.nodes.len(), "unknown output consumer");
        self.edges
            .iter()
            .filter_map(move |&(producer, target)| (target == consumer).then_some(producer))
    }
}

#[cfg(test)]
mod tests {
    use super::OutputGraph;

    fn with_nodes(count: usize) -> OutputGraph {
        let mut graph = OutputGraph::default();
        for index in 0..count {
            assert_eq!(graph.add_node(), index);
        }
        graph
    }

    #[test]
    fn resolves_transitive_dependencies_and_deduplicates_edges() {
        let mut graph = with_nodes(4);
        for edge in [(1, 0), (3, 2), (2, 1), (3, 2), (1, 0)] {
            graph.add_dependency(edge.0, edge.1);
        }
        assert_eq!(graph.resolve(), &[3, 2, 1, 0]);
        assert_eq!(graph.edges.len(), 3);
        assert_eq!(graph.resolve(), &[3, 2, 1, 0]);
        assert!((0..4).all(|index| !graph.is_blocked(index)));
        assert!(graph.dependencies(0).eq([1]));
        assert!(graph.dependencies(3).next().is_none());
    }

    #[test]
    fn diamond_order_is_independent_of_edge_insertion_order() {
        let edges = [(3, 2), (2, 0), (3, 1), (1, 0)];
        let mut forward = with_nodes(4);
        let mut reverse = with_nodes(4);
        for &(producer, consumer) in &edges {
            forward.add_dependency(producer, consumer);
        }
        for &(producer, consumer) in edges.iter().rev() {
            reverse.add_dependency(producer, consumer);
        }
        assert_eq!(forward.resolve(), &[3, 1, 2, 0]);
        assert_eq!(reverse.resolve(), forward.order());
        assert!(forward.dependencies(0).eq([1, 2]));
    }

    #[test]
    fn self_cycle_blocks_its_successors_but_not_an_independent_branch() {
        let mut graph = with_nodes(5);
        for (producer, consumer) in [(0, 0), (0, 1), (1, 2), (3, 4)] {
            graph.add_dependency(producer, consumer);
        }
        assert_eq!(graph.resolve(), &[3, 4]);
        assert!((0..3).all(|index| graph.is_blocked(index)));
        assert!((3..5).all(|index| !graph.is_blocked(index)));
        assert!(graph.dependencies(0).eq([0]));
    }

    #[test]
    fn cycle_blocks_transitive_successors_without_blocking_its_producers() {
        let mut graph = with_nodes(7);
        for (producer, consumer) in [(0, 1), (1, 0), (1, 2), (2, 3), (4, 0), (5, 6)] {
            graph.add_dependency(producer, consumer);
        }
        assert_eq!(graph.resolve(), &[4, 5, 6]);
        assert!((0..4).all(|index| graph.is_blocked(index)));
        assert!((4..7).all(|index| !graph.is_blocked(index)));
        assert!(graph.dependencies(0).eq([1, 4]));
    }

    #[test]
    fn empty_and_disconnected_graphs_keep_node_insertion_order() {
        let mut graph = OutputGraph::default();
        assert!(graph.resolve().is_empty());
        for index in 0..4 {
            assert_eq!(graph.add_node(), index);
        }
        assert_eq!(graph.resolve(), &[0, 1, 2, 3]);
        graph.clear();
        assert!(graph.order().is_empty());
        assert!(graph.resolve().is_empty());
        assert_eq!(graph.add_node(), 0);
        assert_eq!(graph.resolve(), &[0]);
    }

    #[cfg(feature = "std")]
    #[test]
    fn building_prepares_resolution_and_cleared_graphs_reuse_capacity() {
        fn populate(graph: &mut OutputGraph, cyclic: bool) {
            for index in 0..7 {
                assert_eq!(graph.add_node(), index);
            }
            for (producer, consumer) in [(0, 1), (0, 2), (1, 3), (2, 3), (3, 4), (5, 6)] {
                graph.add_dependency(producer, consumer);
            }
            if cyclic {
                graph.add_dependency(5, 5);
            }
        }

        let mut graph = OutputGraph::default();
        populate(&mut graph, true);
        assert_eq!(
            crate::test_support::tracked_allocations(|| {
                assert_eq!(graph.resolve(), &[0, 1, 2, 3, 4]);
            }),
            0
        );
        let capacities = (
            graph.nodes.capacity(),
            graph.edges.capacity(),
            graph.order.capacity(),
        );
        let allocations = crate::test_support::tracked_allocations(|| {
            for turn in 0..64 {
                graph.clear();
                let cyclic = turn % 2 == 0;
                populate(&mut graph, cyclic);
                graph.resolve();
                assert_eq!(graph.order().len(), if cyclic { 5 } else { 7 });
                assert_eq!(graph.is_blocked(5), cyclic);
                assert_eq!(graph.is_blocked(6), cyclic);
                assert!(graph.dependencies(3).eq([1, 2]));
            }
        });
        assert_eq!(allocations, 0);
        assert_eq!(
            (
                graph.nodes.capacity(),
                graph.edges.capacity(),
                graph.order.capacity(),
            ),
            capacities
        );
    }
}
