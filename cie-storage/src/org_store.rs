//! Minimal revision-bound projection for organization merging. Avoid loading
//! summaries, findings and evidence merely to match concept labels/call names.
use cie_core::types::{RevId, SymbolId};
use cie_incremental::snapshot::RevSnapshot;
use cie_store::Store;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, io};

#[derive(Serialize, Deserialize)]
pub struct OrgSymbol {
    pub id: SymbolId,
    pub name: String,
    pub qual_name: String,
}
#[derive(Serialize, Deserialize)]
pub struct OrgConcept {
    pub id: u32,
    pub label: String,
    pub kind: cie_concepts::ConceptKind,
}
#[derive(Serialize, Deserialize)]
pub struct OrgMember {
    pub rev: RevId,
    pub symbols: Vec<OrgSymbol>,
    pub concepts: Vec<OrgConcept>,
    pub unresolved: BTreeMap<SymbolId, Vec<String>>,
}
impl OrgMember {
    pub fn new(snap: &RevSnapshot, unresolved: &BTreeMap<SymbolId, Vec<String>>) -> Self {
        Self {
            rev: snap.rev,
            symbols: snap
                .symbols
                .iter()
                .map(|s| OrgSymbol {
                    id: s.id,
                    name: s.name.clone(),
                    qual_name: s.qual_name.clone(),
                })
                .collect(),
            concepts: snap
                .concepts
                .nodes
                .iter()
                .filter(|n| {
                    // Facet rides with UseCase/Aggregate/Entity: FCA groups are
                    // the label-bearing business concepts of a route-less repo,
                    // and the org-level merge lens needs them to see two repos
                    // sharing a concept. (Whether a FcaSubsumption group may
                    // claim UseCase INSIDE one repo is decided by the dag —
                    // there a shared attribute is never a business story.)
                    matches!(
                        n.kind,
                        cie_concepts::ConceptKind::UseCase
                            | cie_concepts::ConceptKind::Facet
                            | cie_concepts::ConceptKind::Aggregate
                            | cie_concepts::ConceptKind::Entity
                    )
                })
                .map(|n| OrgConcept {
                    id: n.id,
                    label: n.label.clone(),
                    kind: n.kind,
                })
                .collect(),
            unresolved: unresolved.clone(),
        }
    }
}
fn path(store: &Store, rev: RevId) -> std::path::PathBuf {
    store
        .root()
        .join("revs")
        .join(rev.to_string())
        .join("org-member.json")
}
pub fn save(store: &Store, member: &OrgMember) -> io::Result<()> {
    store.write_json(&path(store, member.rev), member)
}
pub fn load(store: &Store, rev: RevId) -> io::Result<Option<OrgMember>> {
    Ok(store
        .read_json::<OrgMember>(&path(store, rev))?
        .filter(|m| m.rev == rev))
}
