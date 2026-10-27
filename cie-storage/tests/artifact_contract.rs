use cie_storage::retrieval::*;
#[test]
fn declared_artifacts_accept_existing_exported_wire_contracts() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("web/public/data");
    for (kind, name) in [
        (ArtifactKind::Report, "report"),
        (ArtifactKind::RepositoryMap, "viewspec"),
        (ArtifactKind::Concepts, "concepts"),
        (ArtifactKind::Dependencies, "deps"),
        (ArtifactKind::Runtime, "runtime"),
        (ArtifactKind::Stories, "stories"),
        (ArtifactKind::Quality, "quality"),
        (ArtifactKind::Overlap, "overlap"),
        (ArtifactKind::Drift, "drift"),
    ] {
        let value =
            serde_json::from_slice(&std::fs::read(root.join(format!("{name}.json"))).unwrap())
                .unwrap();
        let data = OutputData::from_output(
            &Output::Artifact {
                kind,
                availability: AvailabilityPolicy::default(),
                section: None,
                page: PageRequest::default(),
            },
            value,
        );
        assert!(data.is_ok(), "{name}: {data:?}");
    }
}
#[test]
fn artifact_roots_and_discriminators_are_not_arbitrary_json() {
    let malformed = serde_json::from_value::<OutputData>(
        serde_json::json!({"type":"artifact","schema_version":1,"artifact_kind":"report","document":{"arbitrary":42}}),
    );
    assert!(malformed.is_err());
    for kind in ArtifactKind::ALL {
        let value = serde_json::to_value(kind).unwrap();
        assert!(serde_json::from_value::<ArtifactKind>(value).is_ok());
    }
}
