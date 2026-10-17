//! Formal Concept Analysis (PRD §25 / FR-13): incremental Godin lattice
//! construction over (symbols × attributes). Concepts overlap — they are
//! never forced into a single tree (non-goal §4).

use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Concept {
    pub extent: BTreeSet<u32>,
    pub intent: BTreeSet<String>,
    /// Minimal strict super-concepts (parents in lattice order).
    pub parents: Vec<usize>,
}

/// Build the concept lattice incrementally (Godin 1995 style).
/// `objects`: (object id, attribute set). Deterministic for equal input.
pub fn build_lattice(objects: &[(u32, BTreeSet<String>)]) -> Vec<Concept> {
    // Start with the top concept: empty intent (all objects eventually).
    let mut concepts: Vec<Concept> = vec![Concept {
        extent: BTreeSet::new(),
        intent: BTreeSet::new(),
        parents: vec![],
    }];

    for (oid, attrs) in objects {
        let mut to_add: Vec<(BTreeSet<u32>, BTreeSet<String>)> = vec![];
        for c in concepts.iter_mut() {
            if attrs.is_superset(&c.intent) {
                c.extent.insert(*oid);
            } else {
                let inter: BTreeSet<String> = c
                    .intent
                    .intersection(attrs)
                    .cloned()
                    .collect();
                if !inter.is_empty() {
                    let mut ext = c.extent.clone();
                    ext.insert(*oid);
                    to_add.push((ext, inter));
                }
            }
        }
        // The object concept itself.
        to_add.push((BTreeSet::from([*oid]), attrs.clone()));

        for (ext, intent) in to_add {
            if !concepts.iter().any(|c| c.intent == intent) {
                concepts.push(Concept {
                    extent: ext,
                    intent,
                    parents: vec![],
                });
            } else if let Some(existing) = concepts.iter_mut().find(|c| c.intent == intent) {
                existing.extent.extend(ext);
            }
        }
    }

    // Lattice edges: parents = minimal strict supersets of intent
    // (super-concept = more objects, fewer attributes).
    let n = concepts.len();
    for i in 0..n {
        let intent_i = concepts[i].intent.clone();
        let mut parents: Vec<usize> = (0..n)
            .filter(|&j| j != i && concepts[j].intent.is_superset(&intent_i) && !concepts[j].intent.is_subset(&intent_i))
            .filter(|&j| {
                // minimal: no other strict superset between i and j
                !(0..n).any(|k| {
                    k != i
                        && k != j
                        && concepts[k].intent.is_superset(&concepts[i].intent)
                        && !concepts[k].intent.is_subset(&concepts[i].intent)
                        && concepts[j].intent.is_superset(&concepts[k].intent)
                        && !concepts[j].intent.is_subset(&concepts[k].intent)
                })
            })
            .collect();
        parents.sort_unstable();
        concepts[i].parents = parents;
    }

    // Deterministic order: by extent size desc, then intent.
    concepts.sort_by(|a, b| {
        b.extent
            .len()
            .cmp(&a.extent.len())
            .then_with(|| a.intent.cmp(&b.intent))
    });

    concepts
}

/// Human-ish label for a concept from its intent attributes.
pub fn concept_label(intent: &BTreeSet<String>) -> String {
    let mut attrs: Vec<&String> = intent.iter().collect();
    attrs.sort();
    if attrs.is_empty() {
        return "All objects".into();
    }
    let mut names: Vec<String> = vec![];
    for a in attrs.iter().take(3) {
        let a = a.as_str();
        let pretty = match a {
            "db_write" => "writes data",
            "db_read" => "reads data",
            "network" => "calls services",
            "print" => "prints",
            "log" => "logs",
            "crypto" => "uses crypto",
            "auth" => "authorization",
            "money" => "money movement",
            "mutation" => "mutations",
            "query" => "queries",
            "test" => "tests",
            "env_read" => "reads config",
            "eval" => "evaluates code",
            "spawn" => "spawns processes",
            other => other,
        };
        names.push(pretty.to_string());
    }
    if attrs.len() > 3 {
        names.push(format!("+{}", attrs.len() - 3));
    }
    names.join(" & ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(items: &[&str]) -> BTreeSet<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn lattice_finds_overlapping_concepts() {
        let objects = vec![
            (1u32, set(&["db_write", "money"])),
            (2, set(&["db_write", "money", "network"])),
            (3, set(&["print"])),
        ];
        let lattice = build_lattice(&objects);
        // Top concept: empty intent, extent {1,2,3}
        let top = lattice.iter().find(|c| c.intent.is_empty()).unwrap();
        assert_eq!(top.extent.len(), 3);
        // money+db_write concept {1,2} must exist
        let m = lattice
            .iter()
            .find(|c| c.intent == set(&["db_write", "money"]))
            .expect("money concept");
        assert!(m.extent.contains(&1) && m.extent.contains(&2));
        // network concept {2}
        assert!(lattice.iter().any(|c| c.intent.contains("network") && c.extent.contains(&2)));
    }
}
