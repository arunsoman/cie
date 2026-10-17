//! FR-4 byte-stable evidence guard (surfaced by TASK-056's local-fact hash
//! keys): extracting the same source twice — even across processes — must
//! produce byte-identical facts. v0.1 sorted AST children by `node.id()`,
//! which is not stable across parses; this test pins the fix.

use cie_langs::default_registry;

fn serde_facts(facts: &cie_core::facts::LangFacts) -> String {
    serde_json::to_string(facts).unwrap()
}

#[test]
fn extraction_is_byte_stable_within_and_across_runs() {
    let reg = default_registry();
    let src = "import lib\n\ndef authorize(user, token):\n    lib.query_db(\"select\")\n    print(\"ok\")\n    return True\n";
    let first = serde_facts(&reg.adapter_for("auth.py").unwrap().extract("auth.py", src));
    for _ in 0..3 {
        let again = serde_facts(&reg.adapter_for("auth.py").unwrap().extract("auth.py", src));
        assert_eq!(first, again, "same source ⇒ byte-stable facts (FR-4)");
    }

    // Multi-file order independence: extraction order of per-file call lists
    // must not depend on hash-seeded traversal.
    let go = "package main\n\nfunc Run(u string) bool {\n\tLog(u)\n\treturn Authorize(u)\n}\n";
    let g1 = serde_facts(&reg.adapter_for("m.go").unwrap().extract("m.go", go));
    let g2 = serde_facts(&reg.adapter_for("m.go").unwrap().extract("m.go", go));
    assert_eq!(g1, g2);
}
