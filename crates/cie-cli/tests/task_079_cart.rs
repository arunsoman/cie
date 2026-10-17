//! TASK-079 validation — cart fixture + binding sources. Bodies live in
//! `tests/common/mod.rs` for harness dispatch (TASK-084 items 27-31).

mod common;

use common::scenarios;

#[test]
fn task_079_cart_three_level_hierarchy() {
    scenarios::task_079_cart_three_level_hierarchy();
}

#[test]
fn task_079_all_four_binding_sources_attach() {
    scenarios::task_079_all_four_binding_sources_attach();
}

#[test]
fn task_079_llm_candidate_inert() {
    scenarios::task_079_llm_candidate_inert();
}
