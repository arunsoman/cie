//! CIE polyglot layer — PRD v2.0 (docs/prd-v2.0.md) slices 9–20.
//!
//! The honest substrate for the polyglot schema: every table in PRD §11,
//! every row written by a real tree-sitter parse (never synthesized), and
//! every derivation (UAST map, IR lowering, C4, state machines, workflows,
//! duplicates, runtime evidence) carrying the PRD's epistemic encoding.
//!
//! Slice map (PRD §47):
//! * slice 9  — [`schema`]: files/node_kinds/node_text/ast_nodes/symbols
//! * slice 11 — [`uast`]: uast_kinds/uast_map + polyglot queries
//! * slice 10 — [`ir`]: ir_blocks/ir_instructions/ir_edges + the six
//!   sequence-diagram fields on calls + ir_hash/ir_vec
//! * slice 12 — [`frameworks`]: framework_adapters + cross_language_bindings
//! * slice 12b — [`c4`]: containers/components/relationships
//! * slice 13/14 — [`statemachines`]/[`workflows`]: T1/T2/T3 + entry DFS
//! * slice 15 — [`duplicates`]: ir-hash/cosine tiers + canonical rule
//! * slice 20 — [`runtime`]: runtime_evidence + OTel JSON ingest
//!
//! Honesty rules carried over from the rest of the engine:
//! * missing parse = `is_error` row, never silent absence (§13);
//! * no UAST mapping = NULL, the node stays grammar-specific (§14);
//! * every IR instruction carries its `ast_node_id` byte-span back-pointer
//!   (§11.5 honesty invariant);
//! * `Unresolved` is a first-class resolution, never guessed (§9).

pub mod schema;
pub mod uast;
pub mod ir;
pub mod frameworks;
pub mod c4;
pub mod statemachines;
pub mod workflows;
pub mod viewspec;
