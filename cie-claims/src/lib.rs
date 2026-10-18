//! cie-claims — typed claim extraction and the persisted ClaimStore
//! (TASK-072/073). The claim model lives in `model`; extractors and the
//! labeler build on `cie-llm`.

pub mod model;
pub mod share;

pub use share::{share_view, trail_for_reader, covered_elements, DecisionTrail, NoteScope, Position, RedactionRecord, ShareItem, SharedView, TrailEntry, Visibility};
pub mod persist;

pub use persist::{ClaimStore, ClaimStoreExt, StoredClaim};

pub use model::*;
