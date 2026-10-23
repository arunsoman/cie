//! Typed artifact API. Callers supply identity and payload, never paths/codecs.
use cie_core::types::RevId;
use cie_store::Store;
use serde::{de::DeserializeOwned, Serialize};
use std::{io, path::Path};

#[derive(Debug, Clone, Copy)]
pub enum Artifact {
    FileTable,
    Unresolved,
    Stack,
    StateModel,
    Relations,
    Embeddings,
    Polyglot,
    StreamReceipt,
    StageStatus,
    Fault,
}
impl Artifact {
    fn file_name(self) -> &'static str {
        match self {
            Self::FileTable => "file-table.json",
            Self::Unresolved => "unresolved.json",
            Self::Stack => "stack.json",
            Self::StateModel => "state-model.json",
            Self::Relations => "relations.json",
            Self::Embeddings => "embeddings.json",
            Self::Polyglot => "polyglot.json",
            Self::StreamReceipt => "stream.json",
            Self::StageStatus => "stages.json",
            Self::Fault => "fault.json",
        }
    }
}
pub struct RevisionArtifacts {
    store: Store,
}
impl RevisionArtifacts {
    pub fn open(root: &Path) -> io::Result<Self> {
        Ok(Self {
            store: Store::open(root)?,
        })
    }
    pub fn put<T: Serialize>(
        &self,
        revision: RevId,
        kind: Artifact,
        payload: &T,
    ) -> io::Result<()> {
        self.store.write_json(
            &self
                .store
                .root()
                .join("revs")
                .join(revision.to_string())
                .join(kind.file_name()),
            payload,
        )
    }
    pub fn get<T: DeserializeOwned>(
        &self,
        revision: RevId,
        kind: Artifact,
    ) -> io::Result<Option<T>> {
        self.store.read_json(
            &self
                .store
                .root()
                .join("revs")
                .join(revision.to_string())
                .join(kind.file_name()),
        )
    }
}

/// Borrowed-store variants support existing consumers without exposing layout.
pub fn put<T: Serialize>(
    store: &Store,
    revision: RevId,
    kind: Artifact,
    payload: &T,
) -> io::Result<()> {
    store.write_json(
        &store
            .root()
            .join("revs")
            .join(revision.to_string())
            .join(kind.file_name()),
        payload,
    )
}
pub fn get<T: DeserializeOwned>(
    store: &Store,
    revision: RevId,
    kind: Artifact,
) -> io::Result<Option<T>> {
    store.read_json(
        &store
            .root()
            .join("revs")
            .join(revision.to_string())
            .join(kind.file_name()),
    )
}

/// Persist web-compatible outputs once; retrieval never re-runs ingestion.
pub fn put_web_bundle(root: &Path, revision: RevId, payload: &serde_json::Value) -> io::Result<()> {
    let store = Store::open(root)?;
    store.write_json(&store.root().join("revs").join(revision.to_string()).join("web-bundle.json"),payload)
}
