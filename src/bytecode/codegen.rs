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
use crate::bytecode::{END_TRY_MARKER, NO_BLOCK, NO_CONST};

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
            params: &[],
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
    /// The parameter names, in order. Empty for everything but a function or a
    /// method: the body reads its arguments by name, so the names have to be in
    /// the file and not only their count.
    params: &'a [String],
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
            params: decl.params.to_vec(),
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

    /// Compiles `body` into `code`.
    ///
    /// `is_last` says whether `body` is the whole of the block it goes in. It is
    /// only read by a bare expression statement, and it is what makes a block's
    /// value the one its last statement produced — which is what a call returns.
    fn statements(
        &mut self,
        body: &[Stmt],
        code: &mut Vec<Instruction>,
        blocks: &mut Vec<Block>,
        depth: usize,
    ) -> Result<()> {
        let last = body.len().saturating_sub(1);
        for (index, stmt) in body.iter().enumerate() {
            self.statement(stmt, code, blocks, depth, index == last)?;
        }
        Ok(())
    }

    /// `last` says `stmt` is the final statement of its block; see
    /// [`Compiler::statements`].
    fn statement(
        &mut self,
        stmt: &Stmt,
        code: &mut Vec<Instruction>,
        blocks: &mut Vec<Block>,
        depth: usize,
        last: bool,
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
                // The value of a block is the value of its last statement, so a
                // trailing expression leaves what it produced for the block's
                // caller. Anywhere else the value is discarded, and `POP` is how
                // the file says so.
                //
                // An `expect` is the one expression that pushes nothing — it pops
                // both of its operands to compare them, and the tree-walking VM
                // gives an `expect` statement the value `nothing`. A `POP` after
                // one would therefore underflow rather than discard, and the
                // operand stack is already where a discarded value would leave
                // it.
                if !last && !matches!(expr, Expr::Expect { .. }) {
                    emit(code, Opcode::Pop, 0, 0, line);
                }
            }
            Statement::Set { name, value } => {
                self.expr(value, code, line)?;
                let name = self.text(name);
                emit(code, Opcode::Store, name, 0, line);
            }
            // `constant` is its own opcode, not the `STORE` a `set` compiles to: the
            // name is read-only, and the file has to say so for an interpreter
            // to enforce it. A `STORE` naming a constant is what the
            // tree-walking VM refuses at runtime and what `DECLARE_CONST`
            // declares in the file.
            Statement::Constant { name, value } => {
                self.expr(value, code, line)?;
                let name = self.text(name);
                emit(code, Opcode::DeclareConst, name, 0, line);
            }
            Statement::SetProperty {
                object: object_name,
                property,
                value,
            } => {
                let object = self.text(object_name);
                emit(code, Opcode::Load, object, 0, line);
                self.expr(value, code, line)?;
                let target = self.text(&format!("{object_name}.{property}"));
                emit(code, Opcode::SetProperty, target, 0, line);
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
            Statement::Unless { condition, body } => {
                self.expr(condition, code, line)?;
                // There is no `JumpIfTrue`, so the condition is negated and the
                // existing jump runs the body exactly when it was false. The
                // `end` of a one-branch block is that jump's landing pad.
                emit(code, Opcode::Not, 0, 0, line);
                let to_end = jump(code, Opcode::JumpIfFalse, line);
                self.statements(body, code, blocks, depth)?;
                patch_here(code, to_end);
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
                        params,
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
                        params,
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
                        params: &[],
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
                            params: &[],
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
                            params: &[],
                            body: finally_body,
                            span: stmt.span,
                        },
                        depth,
                        blocks,
                    )?
                };
                emit(code, Opcode::Try, catch, finally, line);
                self.statements(body, code, blocks, depth)?;
                // The marked `NOP` closes the protected region: it is where the
                // handlers are popped and the `finally` runs, whether or not the
                // protected code failed. See `Opcode::Nop` and
                // `END_TRY_MARKER`; the operand is what tells it apart from the
                // filler the same byte is everywhere else.
                emit(code, Opcode::Nop, END_TRY_MARKER, 0, line);
            }
            Statement::Import(items) => {
                for item in items {
                    let module = self.text(&item.name);
                    // The name the import binds is a second operand rather than
                    // left to the `STORE` that follows: the importer has to know
                    // *which* name it bound to resolve a call through the alias,
                    // and the `STORE` is the importer's own binding, not the
                    // import's. An import with no alias binds its module's own
                    // name, so the operand is never reserved.
                    let bound = self.text(item.alias.as_ref().unwrap_or(&item.name));
                    emit(code, Opcode::Import, module, bound, line);
                    emit(code, Opcode::Store, bound, 0, line);
                }
            }
            Statement::Module { name, body, .. } => {
                // The declaration is a block of its own, run in a scope and a
                // frame of its own — a `set` inside a module is the module's name,
                // not a name of the program that declared it.
                let mut module_body = Block {
                    name: name.to_string(),
                    kind: BlockKind::Module,
                    arity: 0,
                    params: Vec::new(),
                    code: Vec::new(),
                    blocks: Vec::new(),
                };
                // What the declaration publishes is emitted *in front of* the body,
                // so a jump inside the body still points at the instruction it was
                // compiled for: nothing is inserted after the body was compiled.
                self.module_exports(body, &mut module_body.code, line);
                if depth >= MAX_BLOCK_DEPTH {
                    return Err(Error::Parser(
                        format!("program nests blocks more than {MAX_BLOCK_DEPTH} levels deep"),
                        stmt.span,
                    ));
                }
                self.statements(
                    body,
                    &mut module_body.code,
                    &mut module_body.blocks,
                    depth + 1,
                )?;
                let index = blocks.len() as u32;
                blocks.push(module_body);
                emit(code, Opcode::Module, index, 0, line);
            }
            // An `export` outside a module declaration publishes nothing: there is
            // no module for it to publish into, and the tree-walking VM says so
            // rather than recording it. Inside one, the names are compiled by
            // `Compiler::module_exports` into the run of `EXPORT`s the module's
            // body block opens with, which is where the `MODULE` reads them.
            Statement::Export { .. } => {}
            Statement::Test { name, body } => {
                let block = self.nested(
                    Decl {
                        kind: BlockKind::Test,
                        name,
                        arity: 0,
                        params: &[],
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
                // A receiver that is a plain name is not loaded: `CALL_METHOD`
                // already carries `receiver.method`, and resolves it against the
                // receiver's *name*. Loading it would ask for a binding that
                // need not exist — `json.parse` names a module, and `json` is
                // never a variable. A receiver that is an expression is loaded,
                // because the value is what says whether it is an object at all.
                if !matches!(receiver.as_ref(), Expr::Variable(_)) {
                    self.expr(receiver, code, line)?;
                }
                for arg in args {
                    self.expr(arg, code, line)?;
                }
                let name = self.method_name(receiver, method);
                emit(code, Opcode::CallMethod, name, args.len() as u32, line);
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

    /// Emits the run of `EXPORT`s a `module NAME ... end` declaration opens
    /// with: one per name it publishes.
    ///
    /// The names are decided here, from the same two rules the tree-walking VM
    /// reads them with —
    /// [`module_exports`](crate::parser::module_exports) for which `export`
    /// counts and [`module_declared_names`](crate::parser::module_declared_names)
    /// for what the module defines — so the file says what that VM would have
    /// said for itself. `export all` is expanded to one `EXPORT` per declared
    /// name rather than written as a flag: the names are in the file either way,
    /// and this way a reader of the disassembly sees the list the declaration
    /// publishes.
    ///
    /// An `export` naming a name the module does not define is written with the
    /// reserved `NO_CONST` in its second operand, which is what a VM refuses
    /// rather than publishes. The refusal is *not* made here: the tree-walking
    /// VM reports it when the declaration runs, as a runtime error naming the
    /// module, so refusing it here would answer a different question at a
    /// different time.
    fn module_exports(&mut self, body: &[Stmt], code: &mut Vec<Instruction>, line: u32) {
        let Some((names, all)) = crate::parser::module_exports(body) else {
            // No `export` at all: the module publishes nothing, which is not an
            // error.
            return;
        };
        let declared = crate::parser::module_declared_names(body);
        // `export all` names nothing, so the list it publishes is the declared
        // one — which is what the tree-walking VM publishes for it.
        let names = if all { declared.clone() } else { names };
        for name in names {
            let known = declared.iter().any(|declared| declared == &name);
            let index = self.text(&name);
            emit(
                code,
                Opcode::Export,
                index,
                if all || known { 0 } else { NO_CONST },
                line,
            );
        }
    }

    /// The name a `CALL_METHOD` carries: the dotted `receiver.method` when the
    /// receiver is a name, and the bare method name when it is not.
    ///
    /// The receiver's *name* is what a method call is resolved against, not the
    /// value the receiver happens to hold: `files.read` is the builtin
    /// `files_read` because the receiver is called `files`, and the same spelling
    /// is how `Counter.bump` finds a method on a declared type. A file that
    /// carried only the value would say `read` and leave both of those
    /// unresolvable. A receiver that is an expression rather than a name is the
    /// one case the language rejects, and a bare method name is how the file
    /// says so — see [`crate::bytecode::Opcode::CallMethod`].
    fn method_name(&mut self, receiver: &Expr, method: &str) -> u32 {
        match receiver {
            Expr::Variable(name) => self.text(&format!("{name}.{method}")),
            _ => self.text(method),
        }
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
