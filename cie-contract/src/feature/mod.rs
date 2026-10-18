//! Prompt-to-feature discovery and delivery (CIE-US "Prompt-to-feature
//! discovery and planning" / "Feature implementation and inspection"
//! sections): the engine-side primitives a build-a-feature product layer
//! would drive. Each submodule closes one story; none of them execute
//! code, mutate a real working tree, or apply anything outside its own
//! record.

pub mod request;

pub use request::{Deliverable, DetailLevel, FeatureRequest};
