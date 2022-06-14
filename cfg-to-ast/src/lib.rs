use std::rc::Rc;


use cfg_ir::constant::Constant;

use cfg_ir::instruction::location::{InstructionIndex, InstructionLocation};
use cfg_ir::instruction::value_info::ValueInfo;
use cfg_ir::instruction::{BinaryOp, Inner, Terminator, UnaryOp};

use cfg_ir::value::ValueId;
use fxhash::{FxHashMap, FxHashSet};
use graph::algorithms::dominators::{compute_immediate_dominators, post_dominator_tree};

use cfg_ir::function::Function;
use graph::algorithms::back_edges;
use graph::{Edge, Graph, NodeId};

use ast_ir::{Block, Local};
use local_declaration::LocalDeclaration;

mod local_declaration;

struct Lifter<'a> {
    cfg: &'a Function,
    graph: &'a Graph,
    idoms: &'a FxHashMap<NodeId, NodeId>,
    loop_headers: FxHashSet<NodeId>,
    visited: FxHashSet<NodeId>,
    post_dom_tree: &'a Graph,
    back_edges: FxHashSet<Edge>,
    loop_exits: FxHashSet<NodeId>,
    locals: FxHashMap<ValueId, Rc<Local>>,
    local_declarations: FxHashMap<InstructionLocation, Vec<LocalDeclaration>>,
}

impl<'a> Lifter<'a> {
    fn new(
        cfg: &'a Function,
        graph: &'a Graph,
        entry: NodeId,
        idoms: &'a FxHashMap<NodeId, NodeId>,
        post_dom_tree: &'a Graph,
    ) -> Self {
        let back_edges = back_edges(graph, entry)
            .unwrap()
            .iter()
            .cloned()
            .collect::<FxHashSet<_>>();
        let loop_headers = back_edges
            .iter()
            .map(|edge| edge.destination)
            .collect::<FxHashSet<_>>();
        let local_declarations = local_declaration::local_declarations(cfg, entry);
        Self {
            cfg,
            graph,
            idoms,
            loop_headers,
            visited: FxHashSet::default(),
            post_dom_tree,
            back_edges,
            loop_exits: FxHashSet::default(),
            locals: FxHashMap::default(),
            local_declarations,
        }
    }

    fn follow_edge(&mut self, source: NodeId, destination: NodeId, body: &mut Block) {
        if self.visited.contains(&destination) {
            if self.back_edges.contains(&Edge::new(source, destination)) {
                // uncomment for luau
                /*body.statements
                .push(ast_ir::Stat::Continue(ast_ir::Continue { pos: None }));*/
            } else if self.loop_exits.contains(&destination) {
                body.statements
                    .push(ast_ir::Stat::Break(ast_ir::Break { pos: None }));
            }
        } else {
            self.lift_block(destination, body);
        }
    }

    fn lift_conditional(
        &mut self,
        header: NodeId,
        condition: ValueId,
        true_branch: NodeId,
        false_branch: NodeId,
        body: &mut Block,
    ) {
        let exit_statements = self.post_dom_tree.predecessors(header).next().map(|exit| {
            let mut exit_body = Block::new(None);
            if !self.visited.contains(&exit) {
                self.lift_block(exit, &mut exit_body);
            } else {
                // really we should somehow return None here...
            }
            exit_body
        });
        let mut then_block = Block::new(None);
        self.follow_edge(header, true_branch, &mut then_block);
        let mut else_block = {
            let mut else_block = Block::new(None);
            self.follow_edge(header, false_branch, &mut else_block);
            if !else_block.statements.is_empty() {
                Some(else_block)
            } else {
                None
            }
        };
        let mut condition = self.get_local(condition);
        if let Some(else_block_unwrapped) = &mut else_block {
            if then_block.statements.is_empty() && !else_block_unwrapped.statements.is_empty() {
                std::mem::swap(else_block_unwrapped, &mut then_block);
                condition = ast_ir::Unary {
                    pos: None,
                    op: ast_ir::UnaryOp::LogicalNot,
                    expr: condition.into(),
                }
                .into();
                else_block = None;
            }
        }
        body.statements.push(ast_ir::Stat::If(ast_ir::If {
            pos: None,
            condition,
            then_block,
            else_block,
        }));
        if let Some(exit_statements) = exit_statements {
            body.statements.extend(exit_statements.statements);
        }
    }

    fn get_local(&self, value: ValueId) -> ast_ir::Expr {
        ast_ir::ExprLocal {
            pos: None,
            local: self.locals[&value].clone(),
            prefix: false,
        }
        .into()
    }

    fn convert_constant(constant: &Constant) -> ast_ir::Expr {
        ast_ir::ExprLit {
            pos: None,
            lit: match constant {
                Constant::Nil => ast_ir::Lit::Nil,
                Constant::Boolean(b) => ast_ir::Lit::Boolean(*b),
                Constant::Number(n) => ast_ir::Lit::Number(*n),
                Constant::String(s) => ast_ir::Lit::String(s.clone()),
            },
        }
        .into()
    }

    fn forward_declarations(
        &self,
        location: InstructionLocation,
        body: &mut Block,
    ) -> FxHashSet<ValueId> {
        let mut normal_declarations = FxHashSet::default();
        if let Some(declarations) = self.local_declarations.get(&location) {
            for declaration in declarations {
                if declaration.forward_declare {
                    body.statements.push(
                        ast_ir::Assign {
                            pos: None,
                            vars: vec![ast_ir::ExprLocal {
                                pos: None,
                                local: self.locals[&declaration.value].clone(),
                                prefix: true,
                            }
                            .into()],
                            values: vec![ast_ir::ExprLit {
                                pos: None,
                                lit: ast_ir::Lit::Nil,
                            }
                            .into()],
                        }
                        .into(),
                    );
                } else {
                    normal_declarations.insert(declaration.value);
                }
            }
        }
        normal_declarations
    }

    fn lift_instructions(&mut self, node: NodeId, body: &mut Block) {
        for (index, instruction) in self
            .cfg
            .block(node)
            .unwrap()
            .inner_instructions
            .iter()
            .enumerate()
        {
            let prefix_locals = self.forward_declarations(
                InstructionLocation {
                    node,
                    index: InstructionIndex::Inner(index),
                },
                body,
            );
            let load_local = |value: ValueId| -> ast_ir::Expr {
                ast_ir::ExprLocal {
                    pos: None,
                    local: self.locals[&value].clone(),
                    prefix: prefix_locals.contains(&value),
                }
                .into()
            };
            match instruction {
                Inner::Move(move_value) => {
                    let dest = load_local(move_value.dest);
                    let source = self.get_local(move_value.source);
                    body.statements.push(
                        ast_ir::Assign {
                            pos: None,
                            vars: vec![dest],
                            values: vec![source],
                        }
                        .into(),
                    );
                }
                Inner::LoadConstant(load_constant) => {
                    let dest = load_local(load_constant.dest);
                    let constant = Self::convert_constant(&load_constant.constant);
                    body.statements.push(
                        ast_ir::Assign {
                            pos: None,
                            vars: vec![dest],
                            values: vec![constant],
                        }
                        .into(),
                    );
                }
                Inner::LoadGlobal(load_global) => {
                    let dest = load_local(load_global.dest);
                    let global = ast_ir::Global {
                        pos: None,
                        name: load_global.name.clone(),
                    };
                    body.statements.push(
                        ast_ir::Assign {
                            pos: None,
                            vars: vec![dest],
                            values: vec![global.into()],
                        }
                        .into(),
                    );
                }
                Inner::StoreGlobal(store_global) => {
                    let dest = ast_ir::Global {
                        pos: None,
                        name: store_global.name.clone(),
                    };
                    let value = self.get_local(store_global.value);
                    body.statements.push(
                        ast_ir::Assign {
                            pos: None,
                            vars: vec![dest.into()],
                            values: vec![value.into()],
                        }
                        .into(),
                    );
                }
                Inner::Unary(unary) => {
                    let dest = load_local(unary.dest);
                    let expr = Box::new(self.get_local(unary.value));
                    let op = match unary.op {
                        UnaryOp::LogicalNot => ast_ir::UnaryOp::LogicalNot,
                        UnaryOp::Minus => ast_ir::UnaryOp::Minus,
                        UnaryOp::Len => ast_ir::UnaryOp::Len,
                    };
                    body.statements.push(
                        ast_ir::Assign {
                            pos: None,
                            vars: vec![dest.into()],
                            values: vec![ast_ir::Unary {
                                pos: None,
                                op,
                                expr,
                            }
                            .into()],
                        }
                        .into(),
                    );
                }
                Inner::Binary(binary) => {
                    let dest = load_local(binary.dest);
                    let lhs = Box::new(self.get_local(binary.lhs));
                    let rhs = Box::new(self.get_local(binary.rhs));
                    let op = match binary.op {
                        BinaryOp::Add => ast_ir::BinaryOp::Add,
                        BinaryOp::Sub => ast_ir::BinaryOp::Sub,
                        BinaryOp::Mul => ast_ir::BinaryOp::Mul,
                        BinaryOp::Div => ast_ir::BinaryOp::Div,
                        BinaryOp::Mod => ast_ir::BinaryOp::Mod,
                        BinaryOp::Pow => ast_ir::BinaryOp::Pow,
                        BinaryOp::Equal => ast_ir::BinaryOp::Equal,
                        BinaryOp::LesserOrEqual => ast_ir::BinaryOp::LesserOrEqual,
                        BinaryOp::LesserThan => ast_ir::BinaryOp::LesserThan,
                        BinaryOp::LogicalAnd => ast_ir::BinaryOp::LogicalAnd,
                        BinaryOp::LogicalOr => ast_ir::BinaryOp::LogicalOr,
                    };
                    body.statements.push(
                        ast_ir::Assign {
                            pos: None,
                            vars: vec![dest.into()],
                            values: vec![ast_ir::Binary {
                                pos: None,
                                op,
                                lhs,
                                rhs,
                            }
                            .into()],
                        }
                        .into(),
                    );
                }
                Inner::Concat(concat) => {
                    assert!(
                        concat.values.len() >= 2,
                        "concat requires at least 2 values"
                    );

                    let dest = load_local(concat.dest);
                    let mut value = ast_ir::Binary {
                        pos: None,
                        op: ast_ir::BinaryOp::Concat,
                        lhs: Box::new(self.get_local(concat.values[0])),
                        rhs: Box::new(self.get_local(concat.values[1])),
                    }
                    .into();

                    if concat.values.len() > 2 {
                        for &val in concat.values.iter().skip(2) {
                            value = ast_ir::Binary {
                                pos: None,
                                op: ast_ir::BinaryOp::Concat,
                                lhs: Box::new(value),
                                rhs: Box::new(self.get_local(val)),
                            }
                            .into();
                        }
                    }

                    body.statements.push(
                        ast_ir::Assign {
                            pos: None,
                            vars: vec![dest.into()],
                            values: vec![value],
                        }
                        .into(),
                    );
                }
                _ => println!("Skipped {:?}", instruction),
            }
        }
    }

    fn lift_block_internal(&mut self, node: NodeId, body: &mut Block) {
        /*body.statements.push(ast_ir::Stat::Comment(ast_ir::Comment {
            pos: None,
            comment: node.to_string(),
        }));*/

        self.lift_instructions(node, body);

        self.forward_declarations(
            InstructionLocation {
                node,
                index: InstructionIndex::Terminator,
            },
            body,
        );

        match &self.cfg.block(node).unwrap().terminator() {
            Some(terminator) => match terminator {
                Terminator::UnconditionalJump(jump) => self.follow_edge(node, jump.0, body),
                Terminator::ConditionalJump(jump) => self.lift_conditional(
                    node,
                    jump.condition,
                    jump.true_branch,
                    jump.false_branch,
                    body,
                ),
                Terminator::Return(ret) => {
                    body.statements.push(ast_ir::Stat::Return(ast_ir::Return {
                        pos: None,
                        values: ret
                            .values
                            .iter()
                            .map(|value| self.get_local(*value))
                            .collect(),
                    }));
                }
            },
            _ => {}
        }
    }

    fn block_breaks(body: &Block) -> bool {
        return body.statements.len() == 1 && matches!(body.statements[0], ast_ir::Stat::Break(_));
    }

    fn combine_conditions(first: ast_ir::Expr, second: ast_ir::Expr) -> ast_ir::Expr {
        if let ast_ir::Expr::Lit(lit) = &first {
            if lit.lit == ast_ir::Lit::Boolean(true) {
                return second;
            }
        }
        ast_ir::Binary {
            pos: None,
            op: ast_ir::BinaryOp::LogicalAnd,
            lhs: Box::new(first),
            rhs: Box::new(second),
        }
        .into()
    }

    fn optimize_while(while_stat: &mut ast_ir::While) -> bool {
        let stats = &mut while_stat.body.statements;
        if stats.len() == 1 {
            if let ast_ir::Stat::If(if_stat) = stats.get_mut(0).unwrap() {
                if let Some(else_block) = &if_stat.else_block {
                    if Self::block_breaks(else_block) {
                        while_stat.condition = Self::combine_conditions(
                            while_stat.condition.clone(),
                            if_stat.condition.clone(),
                        );
                        while_stat.body = if_stat.then_block.clone();
                        return true;
                    } else if Self::block_breaks(&if_stat.then_block) {
                        while_stat.condition = Self::combine_conditions(
                            while_stat.condition.clone(),
                            ast_ir::Unary {
                                pos: None,
                                op: ast_ir::UnaryOp::LogicalNot,
                                expr: Box::new(if_stat.condition.clone()),
                            }
                            .into(),
                        );
                        while_stat.body = else_block.clone();
                        return true;
                    }
                }
            }
        }
        false
    }

    fn lift_block(&mut self, node: NodeId, body: &mut Block) {
        self.visited.insert(node);
        if self.loop_headers.contains(&node) {
            let loop_exit = self.post_dom_tree.predecessors(node).next().or_else(|| {
                self.graph
                    .nodes()
                    .iter()
                    .filter(|&&n| self.graph.successors(n).next().is_none())
                    .filter(|&exit| self.idoms[exit] == node)
                    .next()
                    .cloned()
            });
            let exit_statements = loop_exit.map(|exit| {
                let mut exit_body = Block::new(None);
                if !self.visited.contains(&exit) {
                    self.loop_exits.insert(exit);
                    self.lift_block(exit, &mut exit_body);
                } else {
                    // really we should somehow return None here...
                }
                exit_body
            });
            let mut while_body = ast_ir::Block::new(None);
            self.lift_block_internal(node, &mut while_body);
            let mut while_stat = ast_ir::While {
                pos: None,
                condition: ast_ir::ExprLit {
                    pos: None,
                    lit: ast_ir::Lit::Boolean(true),
                }
                .into(),
                body: while_body,
            };
            loop {
                if !Self::optimize_while(&mut while_stat) {
                    break;
                }
            }
            body.statements.push(ast_ir::Stat::While(while_stat));
            if let Some(exit_statements) = exit_statements {
                body.statements.extend(exit_statements.statements);
            }
        } else {
            self.lift_block_internal(node, body);
        }
    }

    fn lift(&mut self, node: NodeId) -> Block {
        self.locals.clear();
        for value in self.cfg.values() {
            self.locals.insert(
                value,
                Rc::new(Local {
                    name: format!("l_{}", value),
                }),
            );
        }
        let mut root = Block::new(None);
        self.lift_block(node, &mut root);
        root
    }
}

pub fn lift(cfg: &Function) {
    let graph = cfg.graph();
    let entry = cfg.entry().unwrap();

    let post_dom_tree = post_dominator_tree(graph, entry).unwrap();
    let idoms = compute_immediate_dominators(graph, entry).unwrap();

    //render_to(&post_dom_tree, &mut std::io::stdout());
    let mut lifter = Lifter::new(cfg, graph, entry, &idoms, &post_dom_tree);

    let mut ast_function = ast_ir::Function::new();
    ast_function.body = lifter.lift(entry);

    println!("{}", ast_ir::formatter::format_ast(&ast_function));
}
