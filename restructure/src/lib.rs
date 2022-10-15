#![feature(let_chains)]

use std::iter;

use cfg::{function::Function, block::BranchType};
use fxhash::{FxHashSet, FxHashMap};
use itertools::Itertools;

use petgraph::{
    algo::dominators::{simple_fast, Dominators},
    stable_graph::{NodeIndex, StableDiGraph, EdgeIndex},
    visit::*,
};

mod conditional;
mod jump;
mod r#loop;

pub fn post_dominators<N: Default, E: Default>(
    graph: &mut StableDiGraph<N, E>,
) -> Dominators<NodeIndex> {
    let exits = graph
        .node_identifiers()
        .filter(|&n| graph.neighbors(n).count() == 0)
        .collect_vec();
    let fake_exit = graph.add_node(Default::default());
    for exit in exits {
        graph.add_edge(exit, fake_exit, Default::default());
    }
    let res = simple_fast(Reversed(&*graph), fake_exit);
    assert!(graph.remove_node(fake_exit).is_some());
    res
}

struct GraphStructurer {
    pub function: Function,
    root: NodeIndex,
    loop_headers: FxHashSet<NodeIndex>,
}

impl GraphStructurer {
    fn new(function: Function) -> Self {
        let root = function.entry().unwrap();
        let mut loop_headers = FxHashSet::default();
        depth_first_search(function.graph(), Some(root), |event| {
            if let DfsEvent::BackEdge(_, header) = event {
                loop_headers.insert(header);
            }
        });

        Self {
            function,
            root,
            loop_headers,
        }
    }

    fn block_is_no_op(block: &ast::Block) -> bool {
        !block.iter().any(|s| s.as_comment().is_none())
    }

    fn try_match_pattern(&mut self, node: NodeIndex, dominators: &Dominators<NodeIndex>) -> bool {
        let successors = self.function.successor_blocks(node).collect_vec();

        //println!("before");
        //cfg::dot::render_to(&self.function, &mut std::io::stdout()).unwrap();

        if self.try_collapse_loop(node, dominators) {
            // println!("matched loop");
            return true;
        }

        let changed = match successors.len() {
            0 => false,
            1 => {
                // remove unnecessary jumps to allow pattern matching
                self.match_jump(node, Some(successors[0]), dominators)
            }
            2 => {
                let (then_edge, else_edge) = self.function.conditional_edges(node).unwrap();
                self.match_conditional(node, then_edge.target(), else_edge.target(), dominators)
            }

            _ => unreachable!(),
        };

        //println!("after");
        //dot::render_to(&self.function, &mut std::io::stdout()).unwrap();

        changed
    }

    fn match_blocks(&mut self) -> bool {
        let dfs = Dfs::new(self.function.graph(), self.root)
            .iter(self.function.graph())
            .collect::<FxHashSet<_>>();
        let mut dfs_postorder = DfsPostOrder::new(self.function.graph(), self.root);
        let dominators = simple_fast(self.function.graph(), self.function.entry().unwrap());

        // cfg::dot::render_to(&self.function, &mut std::io::stdout()).unwrap();

        let mut changed = false;
        while let Some(node) = dfs_postorder.next(self.function.graph()) {
            // println!("matching {:?}", node);
            let matched = self.try_match_pattern(node, &dominators);
            changed |= matched;
            // if matched {
            //     cfg::dot::render_to(&self.function, &mut std::io::stdout()).unwrap();
            // }
        }

        for node in self
            .function
            .graph()
            .node_indices()
            .filter(|node| !dfs.contains(node))
            .collect_vec()
        {
            if self.function.block(node).unwrap().first().and_then(|s| s.as_label()).is_none() {
                self.function.remove_block(node);
            } else {
                let matched = self.try_match_pattern(node, &dominators);
                changed |= matched;
            }
        }

        changed
    }

    fn insert_goto_for_edge(&mut self, edge: EdgeIndex) {
        let (source, target) = self.function.graph().edge_endpoints(edge).unwrap();
        if self.function.graph().edge_weight(edge).unwrap().branch_type == BranchType::Unconditional
            && self.function.predecessor_blocks(target).count() == 1
        {
            assert!(self.function.successor_blocks(source).count() == 1);
            // TODO: this code is repeated in match_jump, move to a new function
            let edges = self.function.remove_edges(target);
            let block = self.function.remove_block(target).unwrap();
            self.function.block_mut(source).unwrap().extend(block.0);
            self.function.set_edges(source, edges);
        } else {
            // TODO: make label an Rc and have a global counter for block name
            let label = ast::Label(format!("l{}", target.index()));
            let target_block = self.function.block_mut(target).unwrap();
            if target_block.first().and_then(|s| s.as_label()).is_none() {
                target_block.insert(0, label.clone().into());
            }
            let goto_block = self.function.new_block();
            self.function
                .block_mut(goto_block)
                .unwrap()
                .push(ast::Goto::new(label).into());

            let edge = self.function.graph_mut().remove_edge(edge).unwrap();
            self.function.graph_mut().add_edge(source, goto_block, edge);
        }
    }

    fn collapse(&mut self) {
        loop {
            while self.match_blocks() {}
            if self.function.graph().node_count() == 1 {
                break;
            }
            // last resort refinement
            let edges = self.function.graph().edge_indices().collect::<Vec<_>>();
            // https://edmcman.github.io/papers/usenix13.pdf
            // we prefer to remove edges whose source does not dominate its target, nor whose target dominates its source
            // TODO: try all possible paths and return the one with the least gotos, i don't think there's any other way
            // to get best output
            let mut changed = false;
            for &edge in &edges {
                let (source, target) = self.function.graph().edge_endpoints(edge).unwrap();
                let dominators = simple_fast(self.function.graph(), self.function.entry().unwrap());
                if dominators.dominators(target).unwrap().contains(&source)
                    || dominators.dominators(source).unwrap().contains(&target)
                {
                    continue;
                }

                self.insert_goto_for_edge(edge);
                changed = self.match_blocks();
                if changed {
                    break;
                }
            }

            if !changed {
                for edge in edges {
                    self.insert_goto_for_edge(edge);
                    changed = self.match_blocks();
                    if changed {
                        break;
                    }
                }
                if !changed {
                    break;
                }
            }
        }
    }

    fn structure(mut self) -> ast::Block {
        self.collapse();
        let node_count = self.function.graph().node_count();
        if node_count != 1 {
            iter::once(
                ast::Comment::new(format!("failed to collapse, total nodes: {}", node_count)).into(),
            )
            .chain(self.function.remove_block(self.root).unwrap().0.into_iter())
            .collect::<Vec<_>>()
            .into()
        } else {
            self.function.remove_block(self.root).unwrap()
        }
    }
}

pub fn lift(function: cfg::function::Function) -> ast::Block {
    GraphStructurer::new(function).structure()
}
