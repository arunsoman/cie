//! Local single-user product workspace. No hosted model or remote mutation is enabled.
pub mod answers;
pub mod api;
pub mod http;
pub mod model;
pub mod repository;
pub mod storage;
#[cfg(feature = "runtime")]
pub mod runtime {
    pub async fn healthy() -> bool {
        true
    }
}

pub mod observations;

pub mod orientation;

pub mod queries;

pub mod plans;

pub mod concept_reviews;

pub mod scenarios;

pub mod profiles;
