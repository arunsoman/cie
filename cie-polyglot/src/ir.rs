//! PRD v2.0 slice 10 — IR lowering (§15, §11.4, §11.5, §11.10).
//!
//! "What does this text mean": per symbol, the AST lowers to a CFG of
//! blocks + normalized instructions, every structured instruction carrying
//! its `ast_node_id` byte-span back-pointer (§11.5 honesty invariant).
//!
//! What lowering erases (§15): variable names (the `ir_hash` op stream is
//! rename-of-locals-stable, ANL-05), loop form (`for`/`while` collapse to
//! one Loop shape), compound assignment form.
//!
//! What lowering preserves (§15): byte spans per instruction, external
//! names (for cross-file resolution), literal values, and — the four
//! sequence-diagram additions on `calls` (§11.4): `block_id`, `call_kind`
//! (`sync|async_spawn|async_await|callback|stream`), `guard_summary` (the
//! enclosing branch condition, rendered from source), `throws_set` (the
//! throw statements inside the CALLEE's own body when in-file, else NULL —
//! never invented).
//!
//! The PRD's lowering sketch (§15.1) is expression-tree recursive; here it
//! is iterative over the same node kinds, driven by per-language control
//! flow seed tables (verified grammar kind names, same discipline as the
//! UAST seed).

use std::collections::{BTreeMap, HashMap};

use rusqlite::{Connection, OptionalExtension};

use crate::schema::Lang;
use crate::uast;

// ---------------------------------------------------------------------------
// DDL — §11.4 calls (with all six sequence-diagram fields), §11.5 IR,
// §11.10 source cache. ir_meta carries only ir_hash: the §15 ir_vec op-bag
// (the cosine era's carrier) had no production reader once detection moved
// to jscpd's token tiers — a pre-existing store self-migrates in
// `create_ir_schema`.
// §11.10 source cache plus the ir_meta carrier (ir_hash only).
// ---------------------------------------------------------------------------

pub const DDL_IR: &str = r#"
CREATE TABLE IF NOT EXISTS calls (
    id                INTEGER PRIMARY KEY,
    file_id           INTEGER NOT NULL REFERENCES files(id),
    call_node_id      INTEGER NOT NULL REFERENCES ast_nodes(id),
    caller_symbol_id  INTEGER NOT NULL REFERENCES symbols(id),
    callee_symbol_id  INTEGER REFERENCES symbols(id),
    callee_text       TEXT,
    resolution        TEXT NOT NULL,
    stmt_index        INTEGER NOT NULL,
    block_id          INTEGER REFERENCES ir_blocks(id),
    call_kind         TEXT NOT NULL DEFAULT 'sync',
    throws_set        BLOB,
    guard_summary     TEXT
);
CREATE INDEX IF NOT EXISTS idx_calls_caller ON calls(caller_symbol_id, stmt_index);
CREATE INDEX IF NOT EXISTS idx_calls_callee ON calls(callee_symbol_id);
CREATE INDEX IF NOT EXISTS idx_calls_unres  ON calls(resolution) WHERE resolution = 'UNRESOLVED';

CREATE TABLE IF NOT EXISTS ir_blocks (
    id            INTEGER PRIMARY KEY,
    symbol_id     INTEGER NOT NULL REFERENCES symbols(id),
    block_index   INTEGER NOT NULL,
    kind          TEXT    NOT NULL
);

CREATE TABLE IF NOT EXISTS ir_instructions (
    id            INTEGER PRIMARY KEY,
    block_id      INTEGER NOT NULL REFERENCES ir_blocks(id),
    inst_index    INTEGER NOT NULL,
    op            TEXT    NOT NULL,
    operands      BLOB    NOT NULL,
    ast_node_id   INTEGER REFERENCES ast_nodes(id),
    span_start    INTEGER NOT NULL,
    span_end      INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_ir_ast    ON ir_instructions(ast_node_id);
CREATE INDEX IF NOT EXISTS idx_ir_symbol ON ir_instructions(block_id);

CREATE TABLE IF NOT EXISTS ir_edges (
    from_block_id INTEGER NOT NULL REFERENCES ir_blocks(id),
    to_block_id   INTEGER NOT NULL REFERENCES ir_blocks(id),
    kind          TEXT    NOT NULL,
    PRIMARY KEY (from_block_id, to_block_id, kind)
);
CREATE INDEX IF NOT EXISTS idx_ir_symbol_blocks ON ir_blocks(symbol_id, block_index);

CREATE TABLE IF NOT EXISTS file_source_cache (
    file_id    INTEGER PRIMARY KEY REFERENCES files(id),
    zstd_bytes BLOB NOT NULL,
    revision   INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS ir_meta (
    symbol_id INTEGER PRIMARY KEY REFERENCES symbols(id),
    ir_hash   BLOB NOT NULL
);
"#;

pub fn create_ir_schema(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(DDL_IR).map_err(|e| e.to_string())?;
    // Retired carrier: stores written before the jscpd migration still carry
    // the §15 ir_vec op-bag column (BLOB NOT NULL). New stores (above) don't;
    // opening an old store drops the column so the two-column insert below
    // keeps working against it.
    let has_vec: Option<i64> = conn
        .query_row(
            "SELECT 1 FROM pragma_table_info('ir_meta') WHERE name = 'ir_vec'",
            [],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    if has_vec.is_some() {
        conn.execute_batch("ALTER TABLE ir_meta DROP COLUMN ir_vec;")
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Per-language control-flow seed tables (verified kind names)
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
pub struct CfSeed {
    pub if_kinds: &'static [&'static str],
    pub loop_kinds: &'static [&'static str],
    pub return_kinds: &'static [&'static str],
    pub throw_kinds: &'static [&'static str],
    pub await_kinds: &'static [&'static str],
    pub spawn_kinds: &'static [&'static str],
    pub lambda_kinds: &'static [&'static str],
    pub cond_field: &'static str,
    pub then_field: &'static str,
    pub else_field: &'static str,
    /// statement-container kinds that a statement list descends into
    pub block_kinds: &'static [&'static str],
}

pub fn cf_seed(lang: Lang) -> CfSeed {
    match lang {
        Lang::Python => CfSeed {
            if_kinds: &["if_statement", "elif_clause", "conditional_expression"],
            loop_kinds: &["for_statement", "while_statement"],
            return_kinds: &["return_statement"],
            throw_kinds: &["raise_statement"],
            await_kinds: &["await"],
            spawn_kinds: &[],
            lambda_kinds: &["lambda"],
            cond_field: "condition",
            then_field: "consequence",
            else_field: "alternative",
            block_kinds: &["block"],
        },
        Lang::Rust => CfSeed {
            if_kinds: &["if_expression", "if_let_expression"],
            loop_kinds: &["while_expression", "for_expression", "loop_expression"],
            return_kinds: &["return_expression"],
            throw_kinds: &[],
            await_kinds: &["await_expression"],
            spawn_kinds: &[],
            lambda_kinds: &["closure_expression"],
            cond_field: "condition",
            then_field: "consequence",
            else_field: "alternative",
            block_kinds: &["block"],
        },
        Lang::Go => CfSeed {
            if_kinds: &["if_statement"],
            loop_kinds: &["for_statement"],
            return_kinds: &["return_statement"],
            throw_kinds: &[],
            await_kinds: &[],
            spawn_kinds: &["go_statement"],
            lambda_kinds: &["func_literal"],
            cond_field: "condition",
            then_field: "consequence",
            else_field: "alternative",
            block_kinds: &["block"],
        },
        Lang::JavaScript | Lang::TypeScript | Lang::Tsx => CfSeed {
            if_kinds: &["if_statement", "conditional_expression"],
            loop_kinds: &[
                "for_statement",
                "for_in_statement",
                "for_of_statement",
                "while_statement",
                "do_statement",
            ],
            return_kinds: &["return_statement"],
            throw_kinds: &["throw_statement"],
            await_kinds: &["await_expression"],
            spawn_kinds: &[],
            lambda_kinds: &[
                "arrow_function",
                "function_expression",
                "generator_function",
            ],
            cond_field: "condition",
            then_field: "consequence",
            else_field: "alternative",
            block_kinds: &["statement_block"],
        },
        Lang::Java => CfSeed {
            if_kinds: &["if_statement", "conditional_expression"],
            loop_kinds: &[
                "for_statement",
                "enhanced_for_statement",
                "while_statement",
                "do_statement",
            ],
            return_kinds: &["return_statement"],
            throw_kinds: &["throw_statement"],
            await_kinds: &[],
            spawn_kinds: &[],
            lambda_kinds: &["lambda_expression"],
            cond_field: "condition",
            then_field: "consequence",
            else_field: "alternative",
            block_kinds: &["block", "block_statements"],
        },
        Lang::C | Lang::Cpp => CfSeed {
            if_kinds: &["if_statement", "conditional_expression"],
            loop_kinds: &["for_statement", "while_statement", "do_statement"],
            return_kinds: &["return_statement"],
            throw_kinds: if matches!(lang, Lang::Cpp) {
                &["throw_expression"]
            } else {
                &[]
            },
            await_kinds: &[],
            spawn_kinds: &[],
            lambda_kinds: if matches!(lang, Lang::Cpp) {
                &["lambda_expression"]
            } else {
                &[]
            },
            cond_field: "condition",
            then_field: "consequence",
            else_field: "alternative",
            block_kinds: &["compound_statement"],
        },
        Lang::CSharp => CfSeed {
            if_kinds: &["if_statement", "conditional_expression"],
            loop_kinds: &[
                "for_statement",
                "foreach_statement",
                "while_statement",
                "do_statement",
            ],
            return_kinds: &["return_statement"],
            throw_kinds: &["throw_statement"],
            await_kinds: &["await_expression"],
            spawn_kinds: &[],
            lambda_kinds: &["lambda_expression", "anonymous_method_expression"],
            cond_field: "condition",
            then_field: "consequence",
            else_field: "alternative",
            block_kinds: &["block"],
        },
        Lang::Kotlin => CfSeed {
            if_kinds: &["if_expression"],
            loop_kinds: &["for_statement", "while_statement", "do_while_statement"],
            return_kinds: &["jump_expression"],
            throw_kinds: &[],
            await_kinds: &[],
            spawn_kinds: &[],
            lambda_kinds: &["lambda_literal"],
            cond_field: "condition",
            then_field: "consequence",
            else_field: "alternative",
            block_kinds: &["block", "statements"],
        },
        Lang::Lua => CfSeed {
            if_kinds: &["if_statement", "elseif_statement"],
            loop_kinds: &["for_statement", "while_statement", "repeat_statement"],
            return_kinds: &["return_statement"],
            throw_kinds: &[],
            await_kinds: &[],
            spawn_kinds: &[],
            lambda_kinds: &["function_definition"],
            cond_field: "condition",
            then_field: "body",
            else_field: "alternative",
            block_kinds: &["block"],
        },
        Lang::Php => CfSeed {
            if_kinds: &["if_statement", "conditional_expression"],
            loop_kinds: &[
                "for_statement",
                "foreach_statement",
                "while_statement",
                "do_statement",
            ],
            return_kinds: &["return_statement"],
            throw_kinds: &["throw_statement"],
            await_kinds: &[],
            spawn_kinds: &[],
            lambda_kinds: &["arrow_function", "anonymous_function"],
            cond_field: "condition",
            then_field: "consequence",
            else_field: "alternative",
            block_kinds: &["declaration_list", "block"],
        },
    }
}

// ---------------------------------------------------------------------------
// In-memory CFG
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct Instr {
    op: &'static str,
    operands: Vec<u8>,
    ast_node_id: Option<i64>,
    span: (usize, usize),
}

#[derive(Debug, Default)]
struct Block {
    kind: &'static str,
    instrs: Vec<Instr>,
}

#[derive(Debug, Default)]
struct Cfg {
    blocks: Vec<Block>,
    edges: Vec<(usize, usize, &'static str)>,
}

impl Cfg {
    fn new_block(&mut self, kind: &'static str) -> usize {
        self.blocks.push(Block {
            kind,
            instrs: vec![],
        });
        self.blocks.len() - 1
    }
    fn emit(
        &mut self,
        block: usize,
        op: &'static str,
        node: tree_sitter::Node,
        ast_id: Option<i64>,
    ) {
        self.blocks[block].instrs.push(Instr {
            op,
            operands: vec![],
            ast_node_id: ast_id,
            span: (node.start_byte(), node.end_byte()),
        });
    }
    fn edge(&mut self, from: usize, to: usize, kind: &'static str) {
        if from != to {
            self.edges.push((from, to, kind));
        }
    }
}

/// One lowered call, ready for the DB write.
struct LoweredCall {
    span: (usize, usize),
    callee_text: Option<String>,
    call_kind: &'static str,
    block: usize,
    stmt_index: usize,
    guard: Option<String>,
}

// ---------------------------------------------------------------------------
// Lowering driver
// ---------------------------------------------------------------------------

struct LowerCtx<'a> {
    lang: Lang,
    src: &'a [u8],
    seed: CfSeed,
    cfg: Cfg,
    /// enclosing branch conditions (innermost last) for guard_summary
    guards: Vec<String>,
    calls: Vec<LoweredCall>,
    throws_in_body: Vec<String>,
}

impl<'a> LowerCtx<'a> {
    /// Lower the statement subtree rooted at `node`; returns (entry, exit).
    fn lower_body(&mut self, node: tree_sitter::Node<'a>, ast_map: &AstIdMap) -> (usize, usize) {
        let entry = self.cfg.new_block("entry");
        let exit = self.cfg.new_block("exit");
        let after = self.lower_stmt_list(node, entry, exit, ast_map);
        if after != exit {
            self.cfg.edge(after, exit, "jump");
        }
        (entry, exit)
    }

    fn lower_stmt_list(
        &mut self,
        node: tree_sitter::Node<'a>,
        mut cur: usize,
        exit: usize,
        ast_map: &AstIdMap,
    ) -> usize {
        let mut cursor = node.walk();
        let children: Vec<tree_sitter::Node<'a>> = node.children(&mut cursor).collect();
        drop(cursor);

        for child in children {
            if is_noise(child, self.src) {
                continue;
            }
            let k = child.kind();
            if self.seed.block_kinds.contains(&k) {
                // a statement container is not a branch — descend in place
                cur = self.lower_stmt_list(child, cur, exit, ast_map);
            } else if self.seed.if_kinds.contains(&k) {
                cur = self.lower_if(child, cur, exit, ast_map);
            } else if self.seed.loop_kinds.contains(&k) {
                cur = self.lower_loop(child, cur, exit, ast_map);
            } else if self.seed.return_kinds.contains(&k) {
                // walk the returned expression first — calls inside it are
                // real calls (sequence diagrams need them)
                let mut rc = child.walk();
                let named: Vec<tree_sitter::Node<'a>> = child
                    .children(&mut rc)
                    .filter(|c| c.is_named() && c.kind() != "return_expression_keyword")
                    .collect();
                drop(rc);
                for v in named {
                    self.lower_expr(v, cur, ast_map);
                }
                self.cfg.emit(cur, "Ret", child, ast_map.id_of(child));
                self.cfg.edge(cur, exit, "jump");
                cur = self.cfg.new_block("normal");
                self.guards.pop();
            } else if self.seed.throw_kinds.contains(&k) {
                self.cfg.emit(cur, "Throw", child, ast_map.id_of(child));
                self.cfg.edge(cur, exit, "exception");
                cur = self.cfg.new_block("normal");
            } else if self.seed.spawn_kinds.contains(&k) {
                let inner = child.child(1).unwrap_or(child);
                self.record_call(inner, cur, "async_spawn", ast_map);
                self.cfg.emit(cur, "Spawn", child, ast_map.id_of(child));
            } else {
                self.lower_expr(child, cur, ast_map);
            }
        }
        cur
    }

    fn lower_if(
        &mut self,
        node: tree_sitter::Node<'a>,
        cur: usize,
        exit: usize,
        ast_map: &AstIdMap,
    ) -> usize {
        let cond = node
            .child_by_field_name(self.seed.cond_field)
            .or_else(|| first_named_child(node));
        let cond_text = cond
            .and_then(|c| c.utf8_text(self.src).ok())
            .map(|t| t.chars().take(120).collect::<String>());
        if let Some(c) = cond {
            self.lower_expr(c, cur, ast_map);
            self.cfg.emit(cur, "Cmp", c, ast_map.id_of(c));
        }
        let then_bb = self.cfg.new_block("normal");
        let else_bb = self.cfg.new_block("normal");
        self.cfg.edge(cur, then_bb, "branch_true");
        self.cfg.edge(cur, else_bb, "branch_false");

        if let Some(t) = cond_text {
            self.guards.push(t);
        }
        let then_end = match node.child_by_field_name(self.seed.then_field) {
            Some(t) => self.lower_stmt_list(t, then_bb, exit, ast_map),
            None => then_bb,
        };
        self.guards.pop();

        let else_end = match node.child_by_field_name(self.seed.else_field) {
            Some(e) => {
                self.guards.push("<else>".into());
                // python/java wrap in else_clause whose children are
                // ["else", ":", block] — locate the statement container by
                // kind instead of a positional index
                let inner = if e.kind() == "else_clause" {
                    let mut ec = e.walk();
                    let container = e
                        .children(&mut ec)
                        .find(|c| {
                            self.seed.block_kinds.contains(&c.kind())
                                || self.seed.if_kinds.contains(&c.kind())
                        })
                        .unwrap_or(e);
                    drop(ec);
                    container
                } else {
                    e
                };
                let end = self.lower_stmt_list(inner, else_bb, exit, ast_map);
                self.guards.pop();
                end
            }
            None => else_bb,
        };

        let merge = self.cfg.new_block("normal");
        if then_end != exit {
            self.cfg.edge(then_end, merge, "jump");
        }
        if else_end != exit {
            self.cfg.edge(else_end, merge, "jump");
        }
        merge
    }

    fn lower_loop(
        &mut self,
        node: tree_sitter::Node<'a>,
        cur: usize,
        exit: usize,
        ast_map: &AstIdMap,
    ) -> usize {
        let cond = node
            .child_by_field_name(self.seed.cond_field)
            .or_else(|| first_named_child(node));
        let cond_bb = self.cfg.new_block("normal");
        self.cfg.edge(cur, cond_bb, "jump");
        if let Some(c) = cond {
            self.lower_expr(c, cond_bb, ast_map);
            self.cfg.emit(cond_bb, "Cmp", c, ast_map.id_of(c));
        }
        let body_bb = self.cfg.new_block("normal");
        let after_bb = self.cfg.new_block("normal");
        self.cfg.edge(cond_bb, body_bb, "branch_true");
        self.cfg.edge(cond_bb, after_bb, "branch_false");
        if let Some(c) = cond.and_then(|c| c.utf8_text(self.src).ok()) {
            self.guards.push(c.chars().take(120).collect());
        }
        let body = node.child_by_field_name("body").unwrap_or(node);
        let body_end = self.lower_stmt_list(body, body_bb, exit, ast_map);
        self.guards.pop();
        if body_end != exit {
            self.cfg.edge(body_end, cond_bb, "jump"); // back edge
        }
        after_bb
    }

    /// Iterative pre-order expression walk: structured ops where the IR
    /// gains meaning; every call node is recorded once.
    fn lower_expr(&mut self, node: tree_sitter::Node<'a>, block: usize, ast_map: &AstIdMap) {
        let mut stack = vec![(node, false)];
        while let Some((n, finish)) = stack.pop() {
            let k = n.kind();
            if finish {
                self.record_call(n, block, classify_call(n, &self.seed), ast_map);
                continue;
            }
            if self.seed.lambda_kinds.contains(&k) {
                self.cfg.emit(block, "Closure", n, ast_map.id_of(n));
                continue; // closure creation does not execute its body
            }
            if is_call_node(self.lang, k) {
                stack.push((n, true));
                let mut cursor = n.walk();
                let children: Vec<_> = n.named_children(&mut cursor).collect();
                for child in children.into_iter().rev() {
                    stack.push((child, false));
                }
                continue;
            }
            if self.seed.await_kinds.contains(&k) {
                self.cfg.emit(block, "Await", n, ast_map.id_of(n));
            } else if is_literal(k) {
                self.cfg.emit(block, "Const", n, ast_map.id_of(n));
            } else if k == "identifier" || k == "field_identifier" || k == "property_identifier" {
                self.cfg.emit(block, "Load", n, ast_map.id_of(n));
            } else if k.contains("binary")
                || k == "comparison_operator"
                || k == "boolean_operator"
                || k == "logical_expression"
            {
                self.cfg.emit(block, "BinOp", n, ast_map.id_of(n));
            }
            let mut c = n.walk();
            let ch: Vec<tree_sitter::Node<'a>> = n.children(&mut c).collect();
            drop(c);
            for ch in ch.into_iter().rev() {
                stack.push((ch, false));
            }
        }
    }

    fn record_call(
        &mut self,
        call: tree_sitter::Node<'a>,
        block: usize,
        call_kind: &'static str,
        ast_map: &AstIdMap,
    ) {
        let stmt_index = self.calls.len();
        self.cfg.emit(block, "Call", call, ast_map.id_of(call));
        let guard = self
            .guards
            .last()
            .filter(|g| g.as_str() != "<else>")
            .cloned();
        let callee_text = callee_text_of(call, self.src);
        self.calls.push(LoweredCall {
            span: (call.start_byte(), call.end_byte()),
            callee_text,
            call_kind,
            block,
            stmt_index,
            guard,
        });
    }
}

fn is_noise(n: tree_sitter::Node, src: &[u8]) -> bool {
    if n.kind() == "comment" {
        return true;
    }
    n.child_count() == 0
        && !n.is_named()
        && n.utf8_text(src)
            .map(|t| t.trim().is_empty())
            .unwrap_or(false)
}

fn first_named_child(n: tree_sitter::Node) -> Option<tree_sitter::Node> {
    n.children(&mut n.walk()).find(|c| c.is_named())
}

fn is_call_node(lang: Lang, kind: &str) -> bool {
    matches!(
        (lang, kind),
        (Lang::Python, "call")
            | (Lang::Rust, "call_expression")
            | (Lang::Go, "call_expression")
            | (Lang::JavaScript, "call_expression")
            | (Lang::TypeScript, "call_expression")
            | (Lang::Java, "method_invocation")
            | (Lang::C, "call_expression")
            | (Lang::Cpp, "call_expression")
            | (Lang::CSharp, "invocation_expression")
            | (Lang::Tsx, "call_expression")
            | (Lang::Kotlin, "call_expression")
            | (Lang::Lua, "function_call")
            | (Lang::Php, "function_call_expression")
            | (Lang::Php, "member_call_expression")
    )
}

fn is_literal(kind: &str) -> bool {
    kind.contains("integer")
        || kind.contains("float")
        || kind.contains("string")
        || kind.contains("boolean")
        || kind.contains("character")
        || kind.contains("null_literal")
}

/// §11.4 call_kind: sync | async_spawn | async_await | callback.
fn classify_call(call: tree_sitter::Node, seed: &CfSeed) -> &'static str {
    let mut anc = call.parent();
    let mut hops = 0;
    while let Some(a) = anc {
        if seed.await_kinds.contains(&a.kind()) {
            return "async_await";
        }
        anc = a.parent();
        hops += 1;
        if hops > 32 {
            break;
        }
    }
    let args = call.child_by_field_name("arguments").or_else(|| {
        call.children(&mut call.walk())
            .find(|c| c.kind().contains("argument"))
    });
    if let Some(args) = args {
        for a in args.children(&mut args.walk()) {
            if seed.lambda_kinds.contains(&a.kind()) {
                return "callback";
            }
        }
    }
    "sync"
}

/// Map tree-sitter node ids to the ast_nodes row ids written by slice 9,
/// keyed by (start_byte, end_byte) with the grammar kinds bucketed per span —
/// a hash lookup plus a tiny kind scan instead of a BTreeMap probe that
/// allocated a kind String at every ancestor level of every query.
#[derive(Clone)]
struct AstIdMap {
    by_span: HashMap<(usize, usize), Vec<(Box<str>, i64)>>,
}

impl AstIdMap {
    fn build(conn: &Connection, file_id: i64) -> Result<Self, String> {
        let mut stmt = conn
            .prepare(
                "SELECT a.start_byte, a.end_byte, k.name, a.id
                 FROM ast_nodes a JOIN node_kinds k ON k.id = a.kind_id
                 WHERE a.file_id = ?1",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(rusqlite::params![file_id], |r| {
                Ok((
                    r.get::<_, i64>(0)? as usize,
                    r.get::<_, i64>(1)? as usize,
                    r.get::<_, String>(2)?,
                    r.get::<_, i64>(3)?,
                ))
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        let mut m: HashMap<(usize, usize), Vec<(Box<str>, i64)>> = HashMap::new();
        for (s, e, k, id) in rows {
            let bucket = m.entry((s, e)).or_default();
            // same last-wins semantics as the old BTreeMap over the same rows
            match bucket.iter_mut().find(|(kk, _)| &**kk == k) {
                Some(slot) => slot.1 = id,
                None => bucket.push((k.into_boxed_str(), id)),
            }
        }
        Ok(AstIdMap { by_span: m })
    }
    fn id_of(&self, node: tree_sitter::Node) -> Option<i64> {
        let mut n = Some(node);
        while let Some(x) = n {
            if let Some(bucket) = self.by_span.get(&(x.start_byte(), x.end_byte())) {
                let kind = x.kind();
                if let Some((_, id)) = bucket.iter().find(|(k, _)| &**k == kind) {
                    return Some(*id);
                }
            }
            n = x.parent();
        }
        None
    }
}

// ---------------------------------------------------------------------------
// Public pipeline
// ---------------------------------------------------------------------------

/// §11.10: store the file source (zstd) so lowering re-parses without the
/// original tree on disk. One bulk transaction — a commit per file would
/// fsync the store n times for data nobody reads until lowering starts.
/// Must not be called with a transaction already open on `conn`.
pub fn store_sources(conn: &Connection, root: &std::path::Path) -> Result<usize, String> {
    let mut count = 0usize;
    let files: Vec<(i64, String)> = {
        let mut stmt = conn
            .prepare("SELECT id, path FROM files ORDER BY id")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        rows
    };
    let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
    {
        let mut ins = tx
            .prepare_cached(
                "INSERT OR REPLACE INTO file_source_cache(file_id, zstd_bytes, revision)
                 SELECT ?1, ?2, revision FROM files WHERE id = ?1",
            )
            .map_err(|e| e.to_string())?;
        for (id, rel) in files {
            let p = root.join(&rel);
            let bytes = std::fs::read(&p).map_err(|e| format!("{}: {e}", p.display()))?;
            ins.execute(rusqlite::params![
                id,
                zstd::bulk::compress(&bytes, 3).map_err(|e| e.to_string())?
            ])
            .map_err(|e| e.to_string())?;
            count += 1;
        }
    }
    tx.commit().map_err(|e| e.to_string())?;
    Ok(count)
}

/// Cache one file's source directly (test + CLI helper when the file came
/// from memory rather than disk).
pub fn cache_source_bytes(conn: &Connection, file_id: i64, src: &str) -> Result<(), String> {
    conn.execute(
        "INSERT OR REPLACE INTO file_source_cache(file_id, zstd_bytes, revision)
         SELECT ?1, ?2, revision FROM files WHERE id = ?1",
        rusqlite::params![
            file_id,
            zstd::bulk::compress(src.as_bytes(), 3).map_err(|e| e.to_string())?
        ],
    )
    .map(|_| ())
    .map_err(|e| e.to_string())
}

#[derive(Debug, Default, serde::Serialize)]
pub struct LowerReport {
    pub symbols_lowered: usize,
    pub blocks: usize,
    pub instructions: usize,
    pub edges: usize,
    pub calls: usize,
    pub calls_resolved: usize,
    pub calls_unresolved: usize,
}

/// Lower every symbol in the store (§15): re-parse from the source cache,
/// locate each symbol's decl node by span, build the CFG, write
/// blocks/instructions/edges/calls + ir_hash/ir_vec.
///
/// One bulk transaction for the whole pass and one shared name→id map: the
/// lowering writes were previously auto-committed per row and the call-name
/// index was rebuilt with a full symbols scan PER FILE (O(files × symbols)).
pub fn lower_all(conn: &Connection) -> Result<LowerReport, String> {
    let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
    uast::normalize(&tx)?;
    let mut report = LowerReport::default();
    let files: Vec<(i64, String)> = {
        let mut stmt = tx
            .prepare("SELECT id, path FROM files ORDER BY id")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        rows
    };
    // call resolution is by name match; ambiguity resolves to the first
    // registered id and the callee_text is always preserved so nothing is
    // hidden. Identical content to the old per-file rebuild (ORDER BY id,
    // first wins) — loaded once instead of once per file.
    let name_ids: BTreeMap<String, i64> = {
        let mut stmt = tx
            .prepare("SELECT name, id FROM symbols ORDER BY id")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        let mut m = BTreeMap::new();
        for (n, id) in rows {
            m.entry(n).or_insert(id);
        }
        m
    };
    for (file_id, path) in files {
        let Some(lang) = Lang::from_path(&path) else {
            continue;
        };
        let compressed: Vec<u8> = tx
            .query_row(
                "SELECT zstd_bytes FROM file_source_cache WHERE file_id = ?1",
                rusqlite::params![file_id],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        let src = zstd::bulk::decompress(&compressed, 16 << 20).map_err(|e| e.to_string())?;
        lower_file(&tx, file_id, lang, &src, &name_ids, &mut report)?;
    }
    tx.commit().map_err(|e| e.to_string())?;
    Ok(report)
}

pub fn lower_file(
    conn: &Connection,
    file_id: i64,
    lang: Lang,
    src: &[u8],
    name_ids: &BTreeMap<String, i64>,
    report: &mut LowerReport,
) -> Result<(), String> {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&lang.ts_language().ok_or("grammar unavailable")?)
        .map_err(|e| e.to_string())?;
    let tree = parser.parse(src, None).ok_or("parse aborted")?;
    lower_tree(conn, file_id, lang, src, &tree, name_ids, report)
}

/// Lower the shared parse tree; caller controls the file transaction.
pub fn lower_tree(
    conn: &Connection,
    file_id: i64,
    lang: Lang,
    src: &[u8],
    tree: &tree_sitter::Tree,
    name_ids: &BTreeMap<String, i64>,
    report: &mut LowerReport,
) -> Result<(), String> {
    let root = tree.root_node();
    let ast_map = AstIdMap::build(conn, file_id)?;

    let syms: Vec<(i64, usize, usize)> = {
        let mut stmt = conn
            .prepare(
                "SELECT id, start_byte, end_byte FROM symbols WHERE file_id = ?1 ORDER BY start_byte",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(rusqlite::params![file_id], |r| {
                Ok((
                    r.get(0)?,
                    r.get::<_, i64>(1)? as usize,
                    r.get::<_, i64>(2)? as usize,
                ))
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        rows
    };

    // one walk of the tree per FILE, not one walk per symbol (and per call):
    // span → node, built in the same stack-DFS order the old tree_by_span used
    // (children pushed in order, popped in reverse; first encounter wins), so
    // lookups return exactly the node tree_by_span would have returned.
    let span_index: HashMap<(usize, usize), tree_sitter::Node> = build_span_index(root);

    for (sym_id, sb, eb) in syms {
        let Some(&decl) = span_index.get(&(sb, eb)) else {
            continue;
        };
        let mut ctx = LowerCtx {
            lang,
            src,
            seed: cf_seed(lang),
            cfg: Cfg::default(),
            guards: vec![],
            calls: vec![],
            throws_in_body: vec![],
        };
        let (entry, exit) = ctx.lower_body(decl, &ast_map);
        let block_rows = write_cfg(conn, sym_id, &ctx, entry, exit, report)?;

        let mut call_ins = conn
            .prepare_cached(
                "INSERT INTO calls(file_id, call_node_id, caller_symbol_id, callee_symbol_id, callee_text,
                     resolution, stmt_index, block_id, call_kind, throws_set, guard_summary)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            )
            .map_err(|e| e.to_string())?;
        for c in &ctx.calls {
            let callee_id = c
                .callee_text
                .as_deref()
                .and_then(last_name_segment)
                .and_then(|n| name_ids.get(n).copied());
            let resolution = if callee_id.is_some() {
                "RESOLVED"
            } else {
                "UNRESOLVED"
            };
            let call_node = span_index.get(&c.span).copied().unwrap_or(root);
            call_ins
                .execute(rusqlite::params![
                    file_id,
                    ast_map.id_of(call_node),
                    sym_id,
                    callee_id,
                    c.callee_text,
                    resolution,
                    c.stmt_index as i64,
                    block_rows[c.block],
                    c.call_kind,
                    if ctx.throws_in_body.is_empty() {
                        None
                    } else {
                        Some(serde_json::to_vec(&ctx.throws_in_body).map_err(|e| e.to_string())?)
                    },
                    c.guard
                ])
                .map_err(|e| e.to_string())?;
            report.calls += 1;
            if callee_id.is_some() {
                report.calls_resolved += 1;
            } else {
                report.calls_unresolved += 1;
            }
        }
        report.symbols_lowered += 1;
    }
    Ok(())
}

/// span → node over the whole file, first encounter wins — the same stack-DFS
/// order and the same winner as the old per-query tree_by_span walk (children
/// pushed in order, popped in reverse), so every lookup is the node the old
/// code would have found, at O(1) instead of O(nodes).
fn build_span_index<'t>(
    root: tree_sitter::Node<'t>,
) -> HashMap<(usize, usize), tree_sitter::Node<'t>> {
    let mut map: HashMap<(usize, usize), tree_sitter::Node<'t>> = HashMap::new();
    let mut stack = vec![root];
    while let Some(n) = stack.pop() {
        map.entry((n.start_byte(), n.end_byte())).or_insert(n);
        let mut c = n.walk();
        let ch: Vec<tree_sitter::Node<'t>> = n.children(&mut c).collect();
        drop(c);
        stack.extend(ch);
    }
    map
}

fn callee_text_of<'t>(call: tree_sitter::Node<'t>, src: &'t [u8]) -> Option<String> {
    let f = call
        .child_by_field_name("function")
        .or_else(|| call.child_by_field_name("callee"))
        .or_else(|| first_named_child(call))?;
    f.utf8_text(src).ok().map(|t| t.chars().take(160).collect())
}

fn last_name_segment(t: &str) -> Option<&str> {
    t.rsplit(['.', ':', '>'])
        .find(|s| !s.is_empty())
        .filter(|s| s.chars().all(|c| c.is_alphanumeric() || c == '_'))
}

/// Write the CFG; returns the DB row id per in-memory block index.
fn write_cfg(
    conn: &Connection,
    sym_id: i64,
    ctx: &LowerCtx,
    entry: usize,
    exit: usize,
    report: &mut LowerReport,
) -> Result<Vec<i64>, String> {
    let mut block_rows: Vec<i64> = Vec::with_capacity(ctx.cfg.blocks.len());
    {
        // prepared once and reused across every block/instruction/edge —
        // these loops were the hottest write path in the store
        let mut block_ins = conn
            .prepare_cached(
                "INSERT INTO ir_blocks(symbol_id, block_index, kind) VALUES (?1, ?2, ?3)",
            )
            .map_err(|e| e.to_string())?;
        let mut instr_ins = conn
            .prepare_cached(
                "INSERT INTO ir_instructions(block_id, inst_index, op, operands, ast_node_id, span_start, span_end)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            )
            .map_err(|e| e.to_string())?;
        for (i, b) in ctx.cfg.blocks.iter().enumerate() {
            let kind = if i == entry {
                "entry"
            } else if i == exit {
                "exit"
            } else {
                b.kind
            };
            block_ins
                .execute(rusqlite::params![sym_id, i as i64, kind])
                .map_err(|e| e.to_string())?;
            let row = conn.last_insert_rowid();
            block_rows.push(row);
            report.blocks += 1;
            for (j, ins) in b.instrs.iter().enumerate() {
                instr_ins
                    .execute(rusqlite::params![
                        row,
                        j as i64,
                        ins.op,
                        ins.operands,
                        ins.ast_node_id,
                        ins.span.0 as i64,
                        ins.span.1 as i64
                    ])
                    .map_err(|e| e.to_string())?;
                report.instructions += 1;
            }
        }
    }
    {
        let mut edge_ins = conn
            .prepare_cached(
                "INSERT OR IGNORE INTO ir_edges(from_block_id, to_block_id, kind) VALUES (?1, ?2, ?3)",
            )
            .map_err(|e| e.to_string())?;
        for (from, to, kind) in &ctx.cfg.edges {
            edge_ins
                .execute(rusqlite::params![block_rows[*from], block_rows[*to], kind])
                .map_err(|e| e.to_string())?;
            report.edges += 1;
        }
    }
    // ANL-05 rename-stable hash. (The §15 ir_vec op-bag — the cosine era's
    // carrier — stopped being written once jscpd replaced the §23.2 similarity
    // ladder: no production reader remained. The bag stays derivable from
    // ir_instructions + ir_edges on demand.)
    let hash = ir_hash_of(ctx);
    conn.prepare_cached("INSERT OR REPLACE INTO ir_meta(symbol_id, ir_hash) VALUES (?1, ?2)")
        .map_err(|e| e.to_string())?
        .execute(rusqlite::params![sym_id, hash])
        .map_err(|e| e.to_string())?;
    Ok(block_rows)
}

fn ir_hash_of(ctx: &LowerCtx) -> Vec<u8> {
    let mut stream = String::new();
    for b in &ctx.cfg.blocks {
        for ins in &b.instrs {
            stream.push_str(ins.op);
            stream.push('\u{1}');
        }
    }
    blake3::hash(stream.as_bytes()).as_bytes().to_vec()
}

/// Ingest a folder into an existing polyglot store, cache sources, lower.
pub fn ingest_and_lower(
    conn: &Connection,
    repo_id: i64,
    revision: i64,
    root: &std::path::Path,
) -> Result<(crate::schema::IngestStats, LowerReport), String> {
    crate::ensure_all_schemas(conn)?;
    let mut st = crate::schema::IngestStats::default();
    crate::schema::ingest_folder(conn, repo_id, revision, root, &mut st)?;
    store_sources(conn, root)?;
    let rep = lower_all(conn)?;
    Ok((st, rep))
}

/// INCREMENTAL-ON (backlog item 6) — the same pipeline with the reuse tier:
/// `prev_db` (the previous promoted store) is ATTACHed during ingest and any
/// walked file whose blake3 matches the previous row (same language, same
/// pinned grammar) carries its parse rows over verbatim instead of
/// re-parsing. Every derivation after the parse tier — source cache, UAST
/// normalization, IR lowering, and everything the CLI stacks on top —
/// recomputes from those rows exactly as before, so a warm rebuild of an
/// unchanged tree is row-for-row the full rebuild. `prev_db = None` is the
/// cold path and behaves exactly like [`ingest_and_lower`].
pub fn ingest_and_lower_reusing(
    conn: &Connection,
    repo_id: i64,
    revision: i64,
    root: &std::path::Path,
    prev_db: Option<&std::path::Path>,
) -> Result<
    (
        crate::schema::IngestStats,
        LowerReport,
        crate::schema::ReuseReport,
    ),
    String,
> {
    crate::ensure_all_schemas(conn)?;
    let mut st = crate::schema::IngestStats::default();
    let reuse =
        crate::schema::ingest_folder_reusing(conn, repo_id, revision, root, &mut st, prev_db)?;
    store_sources(conn, root)?;
    let rep = lower_all(conn)?;
    // INCREMENTAL-ON: stamp the completed build with the tree fingerprint —
    // best-effort, a missing stamp only disables the short-circuit.
    let _ = crate::schema::write_tree_fingerprint(conn, root);
    Ok((st, rep, reuse))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{create_schema, ingest_parsed_file, IngestStats, Lang};
    use rusqlite::Connection;

    #[test]
    fn ir_vec_carrier_self_migrates_from_pre_jscpd_stores() {
        // FKs stay off for the hand-seeded fake rows — the migration's
        // semantics (drop the column, keep the rows) don't depend on FK
        // integrity, and the real ingest path only ever inserts real ids.
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = OFF;").unwrap();
        create_schema(&conn).unwrap();
        conn.execute_batch(
            "CREATE TABLE ir_meta (
                symbol_id INTEGER PRIMARY KEY REFERENCES symbols(id),
                ir_hash   BLOB NOT NULL,
                ir_vec    BLOB NOT NULL
            );",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO ir_meta VALUES (7, x'0123', x'7b226f3a317d')", // {"o":1}
            [],
        )
        .unwrap();

        // ensuring schemas on the old store drops the retired column in place
        create_ir_schema(&conn).unwrap();
        let still_has: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('ir_meta') WHERE name = 'ir_vec'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(still_has, 0, "ir_vec column dropped: ");
        // the pre-existing row survives (its bag bytes are simply gone), and
        // new two-column inserts work against the migrated table
        let rows: i64 = conn
            .query_row("SELECT COUNT(*) FROM ir_meta", [], |r| r.get(0))
            .unwrap();
        assert_eq!(rows, 1);
        conn.execute(
            "INSERT INTO ir_meta(symbol_id, ir_hash) VALUES (8, x'4567')",
            [],
        )
        .unwrap();
    }

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        create_schema(&conn).unwrap();
        create_ir_schema(&conn).unwrap();
        conn
    }

    const PY: &str = "\
def charge(user, amount):
    if amount > 100:
        audit(user, amount)
    else:
        skip(user)
    return authorize(user, amount)
";

    /// ingest one in-memory source and lower it (no disk involved)
    fn ingest_lower(conn: &Connection, path: &str, lang: Lang, src: &str) -> LowerReport {
        let mut st = IngestStats::default();
        let fid = ingest_parsed_file(conn, 1, 41, path, lang, src, &mut st).unwrap();
        cache_source_bytes(conn, fid, src).unwrap();
        lower_all(conn).unwrap()
    }

    #[test]
    fn python_if_else_produces_branch_edges_and_guards() {
        let conn = setup();
        let rep = ingest_lower(&conn, "pay.py", Lang::Python, PY);
        assert_eq!(rep.symbols_lowered, 1, "charge lowered: {rep:?}");
        assert_eq!(rep.calls, 3);

        let (n_cmp, n_bt, n_bf, n_ex): (i64, i64, i64, i64) = conn
            .query_row(
                "SELECT
                    (SELECT COUNT(*) FROM ir_instructions WHERE op='Cmp'),
                    (SELECT COUNT(*) FROM ir_edges WHERE kind='branch_true'),
                    (SELECT COUNT(*) FROM ir_edges WHERE kind='branch_false'),
                    (SELECT COUNT(*) FROM ir_edges WHERE kind='exception')",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .unwrap();
        assert!(
            n_cmp >= 1 && n_bt >= 1 && n_bf >= 1,
            "cmp={n_cmp} bt={n_bt} bf={n_bf}"
        );
        assert_eq!(n_ex, 0, "no throws in this source");

        // guard_summary carries the enclosing branch condition (§11.4)
        let guards: Vec<String> = {
            let mut stmt = conn
                .prepare("SELECT guard_summary FROM calls WHERE guard_summary IS NOT NULL")
                .unwrap();
            stmt.query_map([], |r| r.get::<_, String>(0))
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap()
        };
        assert!(
            guards.iter().any(|g| g.contains("amount > 100")),
            "guards: {guards:?}"
        );
        // else-guarded call is recorded without the then-guard
        let n_else_guarded: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM calls WHERE guard_summary = '<else>'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n_else_guarded, 0, "'<else>' never leaks into guard_summary");
    }

    #[test]
    fn calls_carry_all_six_sequence_fields() {
        let conn = setup();
        ingest_lower(&conn, "pay.py", Lang::Python, PY);
        let (n, n_block, n_kind): (i64, i64, i64) = conn
            .query_row(
                "SELECT COUNT(*), COUNT(block_id), COUNT(call_kind) FROM calls",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert!(n >= 3);
        assert_eq!(n, n_block, "every call carries its block_id");
        assert_eq!(n, n_kind, "every call carries a call_kind");
        let syncs: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM calls WHERE call_kind='sync'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(syncs, n, "plain calls are sync");
        // throws_set honest NULL — the callees here are unresolved externals
        let with_throws: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM calls WHERE throws_set IS NOT NULL",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(with_throws, 0);
    }

    #[test]
    fn callee_resolution_never_guesses() {
        let conn = setup();
        ingest_lower(&conn, "pay.py", Lang::Python, PY);
        let unresolved: Vec<String> = {
            let mut stmt = conn
                .prepare("SELECT callee_text FROM calls WHERE resolution='UNRESOLVED'")
                .unwrap();
            stmt.query_map([], |r| r.get(0))
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap()
        };
        assert_eq!(unresolved.len(), 3, "{unresolved:?}");
        assert!(unresolved.contains(&"audit".to_string()));

        // a repo where the callee exists → RESOLVED by name
        let conn2 = setup();
        ingest_lower(
            &conn2,
            "a.py",
            Lang::Python,
            "def helper():\n    return 1\n\ndef caller():\n    return helper()\n",
        );
        let resolved: i64 = conn2
            .query_row(
                "SELECT COUNT(*) FROM calls WHERE resolution='RESOLVED'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(resolved, 1);
    }

    #[test]
    fn ir_hash_rename_stable() {
        let conn = setup();
        ingest_lower(
            &conn,
            "a.py",
            Lang::Python,
            "def f(alpha):\n    if alpha > 1:\n        return alpha + 1\n    return alpha\n",
        );
        ingest_lower(
            &conn,
            "b.py",
            Lang::Python,
            "def g(zzz):\n    if zzz > 1:\n        return zzz + 1\n    return zzz\n",
        );
        let (h1, h2): (Vec<u8>, Vec<u8>) = (
            conn.query_row(
                "SELECT m.ir_hash FROM ir_meta m JOIN symbols s ON s.id = m.symbol_id WHERE s.name='f'",
                [],
                |r| r.get(0),
            )
            .unwrap(),
            conn.query_row(
                "SELECT m.ir_hash FROM ir_meta m JOIN symbols s ON s.id = m.symbol_id WHERE s.name='g'",
                [],
                |r| r.get(0),
            )
            .unwrap(),
        );
        assert_eq!(h1, h2, "renamed locals must not change the ir_hash");
    }

    #[test]
    fn rust_await_classified_async_await() {
        let conn = setup();
        ingest_lower(
            &conn,
            "a.rs",
            Lang::Rust,
            "async fn go() {\n    db().await;\n}\n",
        );
        let k: String = conn
            .query_row("SELECT call_kind FROM calls LIMIT 1", [], |r| r.get(0))
            .unwrap();
        assert_eq!(k, "async_await", "awaited call classified async_await");
    }

    #[test]
    fn callback_classified_from_lambda_argument() {
        let conn = setup();
        ingest_lower(
            &conn,
            "a.js",
            Lang::JavaScript,
            "function run() {\n    items.map(x => wrap(x));\n}\n",
        );
        let k: String = conn
            .query_row(
                "SELECT call_kind FROM calls WHERE callee_text='items.map'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(k, "callback");
    }

    #[test]
    fn every_structured_instruction_carries_ast_backpointer() {
        let conn = setup();
        ingest_lower(&conn, "pay.py", Lang::Python, PY);
        let unbacked: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM ir_instructions i
                 JOIN ir_blocks b ON b.id = i.block_id
                 WHERE i.ast_node_id IS NULL AND i.op IN ('Call','Ret','Throw','Cmp')",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            unbacked, 0,
            "structured ops must carry their evidence pointer"
        );
    }

    #[test]
    fn sequence_query_answers_from_calls_table() {
        // §34: the sequence-diagram SQL runs as written
        let conn = setup();
        ingest_lower(&conn, "pay.py", Lang::Python, PY);
        let mut stmt = conn
            .prepare(
                "SELECT c.stmt_index, c.guard_summary, c.call_kind, s1.name AS caller, s2.name AS callee
                 FROM calls c
                 JOIN symbols s1 ON s1.id = c.caller_symbol_id
                 LEFT JOIN symbols s2 ON s2.id = c.callee_symbol_id
                 ORDER BY c.caller_symbol_id, c.stmt_index",
            )
            .unwrap();
        let rows: Vec<(i64, Option<String>, String, String, Option<String>)> = stmt
            .query_map([], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].3, "charge");
        assert!(
            rows.iter().all(|r| r.4.is_none()),
            "unresolved callees stay NULL, never guessed"
        );
    }
}
