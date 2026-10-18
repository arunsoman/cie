//! A feature request accepted at any level of detail (US-104): the
//! product never requires internal framework or component names to
//! begin, and every request names its target repository and desired
//! deliverable up front.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Deliverable {
    Plan,
    PreviewCandidate,
    DraftPullRequest,
}

/// How much detail the requester supplied — all equally valid entry points.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DetailLevel {
    Prompt,
    Brief,
    Specification,
    Examples,
}

/// A raw feature request. There is deliberately no field for an internal
/// framework, module or component name: the struct cannot require what
/// the story forbids requiring.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeatureRequest {
    pub id: String,
    pub repository: String,
    pub deliverable: Deliverable,
    pub detail_level: DetailLevel,
    pub raw_text: String,
}

impl FeatureRequest {
    pub fn new(
        id: impl Into<String>,
        repository: impl Into<String>,
        deliverable: Deliverable,
        detail_level: DetailLevel,
        raw_text: impl Into<String>,
    ) -> Self {
        FeatureRequest {
            id: id.into(),
            repository: repository.into(),
            deliverable,
            detail_level,
            raw_text: raw_text.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_a_bare_prompt_as_a_complete_request() {
        let req = FeatureRequest::new("r1", "acme/checkout", Deliverable::Plan, DetailLevel::Prompt, "let guests checkout without an account");
        assert_eq!(req.repository, "acme/checkout");
        assert_eq!(req.deliverable, Deliverable::Plan);
    }

    #[test]
    fn accepts_a_detailed_specification_the_same_way() {
        let req = FeatureRequest::new(
            "r2",
            "acme/checkout",
            Deliverable::DraftPullRequest,
            DetailLevel::Specification,
            "Given a cart with >=1 item, when guest checkout is enabled...",
        );
        assert_eq!(req.detail_level, DetailLevel::Specification);
    }
}
