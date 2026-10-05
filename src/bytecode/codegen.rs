//! Lowering: Redblue's AST to bytecode.
//!
//! One pass, in source order, into a single instruction list per block. The
//! output is a function of the input alone — no hash-map iteration, no clock,
//! no address — so compiling the same program twice gives the same bytes.
//!
//! Names are compiled as names, not as slot numbers. That is deliberate: a
//! Redblue function closes over the scopes live where it was declared, so a
//! name has to survive into the `.rbc` for a VM to resolve it against the
//! captured scope. Resolving names to slots is a later stage and a version
//! bump.

use indexmap::IndexMap;

use crate::error::{Error, Result, Span};
use crate::parser::{BinaryOp, Expr, Program, Statement, Stmt, UnaryOp};

use crate::bytecode::format::{Block, BlockKind, Chunk, Constant, Instruction, MAX_BLOCK_DEPTH};
use crate::bytecode::opcode::Opcode;
use crate::bytecode::{NO_BLOCK, NO_CONST};

/// The name a `repeat ... times` loop counts in.
///
/// A Redblue identifier cannot contain `$`, so a name the compiler introduces
/// cannot collide with one the program declared.
const REPEAT_COUNTER: &str = "$counter";

/// Compiles a program that has already passed the frontend.
pub fn compile(program: &Program) -> Result<Chunk> {
    let mut compiler = Compiler {
        constants: Vec::new(),
        names: IndexMap::new(),
    };

    let main = compiler.block(
        Decl {
            kind: BlockKind::Main,
            name: "main",
            arity: 0,
            body: &program.statements,
            span: Span::unknown(),
        },
        0,
    )?;

    Ok(Chunk {
        constants: compiler.constants,
        main,
    })
}

/// The declaration a block is compiled from.
///
/// `main` is one of these too, which is why the compiler has one entry point
/// rather than a separate path for the program's own code.
struct Decl<'a> {
    kind: BlockKind,
    name: &'a str,
    /// How many parameters the block takes. `0` for everything but a function,
    /// method or test.
    arity: usize,
    body: &'a [Stmt],
    span: Span,
}

struct Compiler {
    constants: Vec<Constant>,
    /// Text constants already in the pool, so a name used a hundred times is
    /// one entry and the pool is the same whichever way round the source used
    /// them.
    names: IndexMap<String, u32>,
}

impl Compiler {
    /// Interns `text` and returns its index.
    fn text(&mut self, text: &str) -> u32 {
        if let Some(index) = self.names.get(text) {
            return *index;
        }
        let index = self.constants.len() as u32;
        self.constants.push(Constant::Text(text.to_string()));
        self.names.insert(text.to_string(), index);
        index
    }

    /// Compiles a declaration into a block of its own.
    fn block(&mut self, decl: Decl<'_>, depth: usize) -> Result<Block> {
        if depth > MAX_BLOCK_DEPTH {
            return Err(Error::Parser(
                format!("program nests blocks more than {MAX_BLOCK_DEPTH} levels deep"),
                decl.span,
            ));
        }

        let mut code = Vec::new();
        let mut blocks = Vec::new();
        self.statements(decl.body, &mut code, &mut blocks, depth)?;

        Ok(Block {
            name: decl.name.to_string(),
            kind: decl.kind,
            arity: decl.arity as u32,
            code,
            blocks,
        })
    }

    /// Compiles a nested declaration, adds it to `blocks` and returns its
    /// index.
    ///
    /// The index is what the parent instruction carries, so `blocks` is in the
    /// order the compiler emitted the bodies in.
    fn nested(&mut self, decl: Decl<'_>, depth: usize, blocks: &mut Vec<Block>) -> Result<u32> {
        let block = self.block(decl, depth + 1)?;
        let index = blocks.len() as u32;
        blocks.push(block);
        Ok(index)
    }

    fn statements(
        &mut self,
        body: &[Stmt],
        code: &mut Vec<Instruction>,
        blocks: &mut Vec<Block>,
        depth: usize,
    ) -> Result<()> {
        for stmt in body {
            self.statement(stmt, code, blocks, depth)?;
        }
        Ok(())
    }

    fn statement(
        &mut self,
        stmt: &Stmt,
        code: &mut Vec<Instruction>,
        blocks: &mut Vec<Block>,
        depth: usize,
    ) -> Result<()> {
        let line = stmt.span.line as u32;

        match &stmt.statement {
            Statement::Say(expr) => {
                self.expr(expr, code, line)?;
                emit(code, Opcode::Say, 0, 0, line);
            }
            Statement::Print(expr) => {
                self.expr(expr, code, line)?;
                emit(code, Opcode::Print, 0, 0, line);
            }
            Statement::Expr(expr) => {
                self.expr(expr, code, line)?;
                emit(code, Opcode::Pop, 0, 0, line);
            }
            Statement::Set { name, value } => {
                self.expr(value, code, line)?;
                let name = self.text(name);
                emit(code, Opcode::Store, name, 0, line);
            }
            Statement::SetProperty {
                object,
                property,
                value,
            } => {
                let object = self.text(object);
                emit(code, Opcode::Load, object, 0, line);
                self.expr(value, code, line)?;
                let property = self.text(property);
                emit(code, Opcode::SetProperty, property, 0, line);
            }
            Statement::If {
                condition,
                then_branch,
                else_branch,
            } => {
                self.expr(condition, code, line)?;
                // The `end` of an `if` with no `else` is where a false
                // condition lands, so there is one jump either way.
                let to_else = jump(code, Opcode::JumpIfFalse, line);
                self.statements(then_branch, code, blocks, depth)?;
                if else_branch.is_empty() {
                    patch_here(code, to_else);
                } else {
                    let to_end = jump(code, Opcode::Jump, line);
                    let else_start = code.len() as u32;
                    patch(code, to_else, else_start);
                    self.statements(else_branch, code, blocks, depth)?;
                    patch_here(code, to_end);
                }
            }
            Statement::ForEach {
                variable,
                iterable,
                body,
            } => {
                self.expr(iterable, code, line)?;
                emit(code, Opcode::GetIter, 0, 0, line);
                let top = code.len() as u32;
                let variable = self.text(variable);
                emit(code, Opcode::Store, variable, 0, line);
                self.statements(body, code, blocks, depth)?;
                emit(code, Opcode::Jump, top, 0, line);
            }
            Statement::ForRange {
                variable,
                start,
                end,
                step,
                body,
            } => {
                self.expr(start, code, line)?;
                self.expr(end, code, line)?;
                let arity = match step {
                    Some(step) => {
                        self.expr(step, code, line)?;
                        3
                    }
                    None => 2,
                };
                emit(code, Opcode::GetRange, 0, arity, line);
                let top = code.len() as u32;
                let variable = self.text(variable);
                emit(code, Opcode::Store, variable, 0, line);
                self.statements(body, code, blocks, depth)?;
                emit(code, Opcode::Jump, top, 0, line);
            }
            Statement::Repeat { count, body } => {
                self.expr(count, code, line)?;
                emit(code, Opcode::GetRange, 0, 1, line);
                let top = code.len() as u32;
                let counter = self.text(REPEAT_COUNTER);
                emit(code, Opcode::Store, counter, 0, line);
                self.statements(body, code, blocks, depth)?;
                emit(code, Opcode::Jump, top, 0, line);
            }
            Statement::While { condition, body } => {
                let top = code.len() as u32;
                self.expr(condition, code, line)?;
                let to_end = jump(code, Opcode::JumpIfFalse, line);
                self.statements(body, code, blocks, depth)?;
                emit(code, Opcode::Jump, top, 0, line);
                patch_here(code, to_end);
            }
            Statement::Break => emit(code, Opcode::Break, 0, 0, line),
            Statement::Skip => emit(code, Opcode::Skip, 0, 0, line),
            Statement::Return(expr) | Statement::GiveBack(expr) => {
                match expr {
                    Some(expr) => self.expr(expr, code, line)?,
                    None => {
                        let nothing = self.constant(Constant::Nothing);
                        emit(code, Opcode::PushConst, nothing, 0, line);
                    }
                }
                emit(code, Opcode::Return, 0, 0, line);
            }
            Statement::Function { name, params, body } => {
                let block = self.nested(
                    Decl {
                        kind: BlockKind::Function,
                        name,
                        arity: params.len(),
                        body,
                        span: stmt.span,
                    },
                    depth,
                    blocks,
                )?;
                emit(code, Opcode::DefFunction, block, params.len() as u32, line);
                let name = self.text(name);
                emit(code, Opcode::Store, name, 0, line);
            }
            Statement::Method { name, params, body } => {
                let block = self.nested(
                    Decl {
                        kind: BlockKind::Method,
                        name,
                        arity: params.len(),
                        body,
                        span: stmt.span,
                    },
                    depth,
                    blocks,
                )?;
                emit(code, Opcode::DefMethod, block, params.len() as u32, line);
            }
            Statement::Has { name, default } => {
                // The initial value is compiled in front of the declaration
                // that consumes it, so a `default` may be any expression
                // rather than only a literal the pool could hold. A field with
                // no `default` is initialised to `nothing`, which is what the
                // tree-walking VM gives it.
                match default {
                    Some(expr) => self.expr(expr, code, line)?,
                    None => {
                        let nothing = self.constant(Constant::Nothing);
                        emit(code, Opcode::PushConst, nothing, 0, line);
                    }
                }
                let name = self.text(name);
                emit(code, Opcode::DefField, name, 0, line);
            }
            Statement::Object {
                name,
                extends,
                body,
            } => {
                let block = self.nested(
                    Decl {
                        kind: BlockKind::Object,
                        name,
                        arity: 0,
                        body,
                        span: stmt.span,
                    },
                    depth,
                    blocks,
                )?;
                // The parent goes in as a name, interned like every other name
                // in the program: a flag saying "this one extends something"
                // would leave the file unable to say what.
                let parent = match extends {
                    Some(parent) => self.text(parent),
                    None => NO_CONST,
                };
                emit(code, Opcode::DefObject, block, parent, line);
                let name = self.text(name);
                emit(code, Opcode::Store, name, 0, line);
            }
            Statement::Try {
                body,
                catch_var,
                catch_body,
                finally_body,
            } => {
                // Handlers become blocks of their own, so the protected code
                // stays a straight run of instructions with no jump patching.
                let catch = if catch_var.is_some() || !catch_body.is_empty() {
                    let name = catch_var.clone().unwrap_or_default();
                    self.nested(
                        Decl {
                            kind: BlockKind::CatchBody,
                            name: &name,
                            arity: 0,
                            body: catch_body,
                            span: stmt.span,
                        },
                        depth,
                        blocks,
                    )?
                } else {
                    NO_BLOCK
                };
                let finally = if finally_body.is_empty() {
                    NO_BLOCK
                } else {
                    self.nested(
                        Decl {
                            kind: BlockKind::FinallyBody,
                            name: "",
                            arity: 0,
                            body: finally_body,
                            span: stmt.span,
                        },
                        depth,
                        blocks,
                    )?
                };
                emit(code, Opcode::Try, catch, finally, line);
                self.statements(body, code, blocks, depth)?;
            }
            Statement::Import(items) => {
                for item in items {
                    let module = self.text(&item.name);
                    emit(code, Opcode::Import, module, 0, line);
                    let bound = self.text(item.alias.as_ref().unwrap_or(&item.name));
                    emit(code, Opcode::Store, bound, 0, line);
                }
            }
            Statement::Test { name, body } => {
                let block = self.nested(
                    Decl {
                        kind: BlockKind::Test,
                        name,
                        arity: 0,
                        body,
                        span: stmt.span,
                    },
                    depth,
                    blocks,
                )?;
                emit(code, Opcode::Test, block, 0, line);
            }
        }

        Ok(())
    }

    fn expr(&mut self, expr: &Expr, code: &mut Vec<Instruction>, line: u32) -> Result<()> {
        match expr {
            Expr::Number(n) => {
                let index = self.constant(Constant::Number(*n));
                emit(code, Opcode::PushConst, index, 0, line);
            }
            Expr::Text(text) => {
                let index = self.constant(Constant::Text(text.clone()));
                emit(code, Opcode::PushConst, index, 0, line);
            }
            Expr::YesNo(b) => {
                let index = self.constant(Constant::YesNo(*b));
                emit(code, Opcode::PushConst, index, 0, line);
            }
            Expr::Nothing => {
                let index = self.constant(Constant::Nothing);
                emit(code, Opcode::PushConst, index, 0, line);
            }
            Expr::Variable(name) => {
                let name = self.text(name);
                emit(code, Opcode::Load, name, 0, line);
            }
            Expr::Binary { op, left, right } => {
                self.expr(left, code, line)?;
                self.expr(right, code, line)?;
                emit(code, binary_opcode(op), 0, 0, line);
            }
            Expr::Unary { op, expr: inner } => {
                self.expr(inner, code, line)?;
                emit(
                    code,
                    match op {
                        UnaryOp::Neg => Opcode::Neg,
                        UnaryOp::Not => Opcode::Not,
                    },
                    0,
                    0,
                    line,
                );
            }
            Expr::Call { name, args } => {
                for arg in args {
                    self.expr(arg, code, line)?;
                }
                let name = self.text(name);
                emit(code, Opcode::Call, name, args.len() as u32, line);
            }
            Expr::MethodCall {
                receiver,
                method,
                args,
            } => {
                self.expr(receiver, code, line)?;
                for arg in args {
                    self.expr(arg, code, line)?;
                }
                let method = self.text(method);
                emit(code, Opcode::CallMethod, method, args.len() as u32, line);
            }
            Expr::Property { object, property } => {
                self.expr(object, code, line)?;
                let property = self.text(property);
                emit(code, Opcode::LoadProperty, property, 0, line);
            }
            Expr::Index { object, index } => {
                self.expr(object, code, line)?;
                self.expr(index, code, line)?;
                emit(code, Opcode::Index, 0, 0, line);
            }
            Expr::InterpolatedText(parts) => {
                for part in parts {
                    self.expr(part, code, line)?;
                }
                emit(code, Opcode::BuildText, parts.len() as u32, 0, line);
            }
            Expr::List(items) => {
                for item in items {
                    self.expr(item, code, line)?;
                }
                emit(code, Opcode::BuildList, items.len() as u32, 0, line);
            }
            Expr::Record(fields) => {
                for (key, value) in fields {
                    let key = self.text(key);
                    emit(code, Opcode::PushConst, key, 0, line);
                    self.expr(value, code, line)?;
                }
                emit(code, Opcode::BuildRecord, fields.len() as u32, 0, line);
            }
            Expr::Expect { actual, expected } => {
                self.expr(actual, code, line)?;
                self.expr(expected, code, line)?;
                emit(code, Opcode::Expect, 0, 0, line);
            }
        }

        Ok(())
    }

    fn constant(&mut self, constant: Constant) -> u32 {
        let index = self.constants.len() as u32;
        self.constants.push(constant);
        index
    }
}

fn binary_opcode(op: &BinaryOp) -> Opcode {
    match op {
        BinaryOp::Add => Opcode::Add,
        BinaryOp::Sub => Opcode::Sub,
        BinaryOp::Mul => Opcode::Mul,
        BinaryOp::Div => Opcode::Div,
        BinaryOp::Mod => Opcode::Mod,
        BinaryOp::Equal => Opcode::Equal,
        BinaryOp::NotEqual => Opcode::NotEqual,
        BinaryOp::Less => Opcode::Less,
        BinaryOp::LessEqual => Opcode::LessEqual,
        BinaryOp::Greater => Opcode::Greater,
        BinaryOp::GreaterEqual => Opcode::GreaterEqual,
        BinaryOp::And => Opcode::And,
        BinaryOp::Or => Opcode::Or,
        BinaryOp::In => Opcode::In,
    }
}

fn emit(code: &mut Vec<Instruction>, opcode: Opcode, arg: u32, aux: u32, line: u32) {
    code.push(Instruction {
        opcode,
        arg,
        aux,
        line,
    });
}

/// Appends a jump whose target is not known yet and returns the index to patch.
fn jump(code: &mut Vec<Instruction>, opcode: Opcode, line: u32) -> usize {
    emit(code, opcode, 0, 0, line);
    code.len() - 1
}

/// Points a jump at `target`, which is the offset of an instruction in the
/// same block.
fn patch(code: &mut [Instruction], at: usize, target: u32) {
    code[at].arg = target;
}

/// Points a jump at the next instruction to be emitted.
fn patch_here(code: &mut [Instruction], at: usize) {
    let end = code.len() as u32;
    patch(code, at, end);
}
