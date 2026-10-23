use cie_adapters::LanguageAdapter;
use cie_storage::{
    FileSubmission, RepositoryReader, RepositoryWriter, StoragePolicy, SymbolQuery, WorkloadProfile,
};
use std::path::PathBuf;
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "cie-storage-api-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        Self(root)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let db = cie_polyglot::schema::default_db_path(&self.0);
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{suffix}", db.display()));
        }
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn submit(writer: &mut RepositoryWriter, path: &str, source: &str) {
    let adapter = cie_adapter_python::PythonAdapter;
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&adapter.grammar(path).unwrap())
        .unwrap();
    let tree = parser.parse(source, None).unwrap();
    let facts = adapter.extract_tree(path, source, &tree);
    writer
        .put_file(FileSubmission {
            path,
            hash: cie_core::types::content_hash(source.as_bytes()),
            source,
            facts,
            tree: Some(&tree),
        })
        .unwrap();
}
fn publish(root: &std::path::Path, source: &str, rev: u64) {
    let mut writer = RepositoryWriter::open(root, StoragePolicy::default()).unwrap();
    submit(&mut writer, "app.py", source);
    let graph = writer.materialize().unwrap();
    writer
        .commit(rev, &graph, &serde_json::json!({}), &[])
        .unwrap();
}
#[test]
fn typed_reads_are_bounded_and_aborted_writes_do_not_replace_published_data() {
    let f = Fixture::new();
    publish(
        &f.0,
        "def a():\n    return 1\ndef b():\n    return 2\ndef c():\n    return 3\n",
        1,
    );
    let mut policy = StoragePolicy::for_workload(WorkloadProfile::LowMemory);
    policy.max_page_size = 2;
    let reader = RepositoryReader::open(&f.0, policy).unwrap();
    assert_eq!(
        reader
            .symbols(&SymbolQuery {
                limit: 100,
                ..Default::default()
            })
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        reader
            .symbols(&SymbolQuery {
                name: Some("b".into()),
                ..Default::default()
            })
            .unwrap()[0]
            .revision,
        1
    );
    {
        let mut draft = RepositoryWriter::open(&f.0, StoragePolicy::default()).unwrap();
        submit(&mut draft, "app.py", "def draft_only():\n    return 4\n");
        // No commit: dropping the transaction rolls back and removes scratch data.
    }
    let current = RepositoryReader::open(&f.0, StoragePolicy::default()).unwrap();
    assert!(current
        .symbols(&SymbolQuery {
            name: Some("draft_only".into()),
            ..Default::default()
        })
        .unwrap()
        .is_empty());
    assert!(current
        .file_facts("app.py")
        .unwrap()
        .unwrap()
        .symbols
        .iter()
        .any(|s| s.name == "a"));
    publish(&f.0, "def replacement():\n    return 5\n", 2);
    let prior = reader
        .symbols(&SymbolQuery {
            name: Some("b".into()),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(prior.len(), 1);
    assert_eq!(prior[0].revision, 1);
    let current = RepositoryReader::open(&f.0, StoragePolicy::default()).unwrap();
    assert_eq!(
        current
            .symbols(&SymbolQuery {
                name: Some("replacement".into()),
                ..Default::default()
            })
            .unwrap()[0]
            .revision,
        2
    );
}
#[test]
fn storage_validates_identity_and_owns_policy_limits() {
    let f = Fixture::new();
    let mut writer = RepositoryWriter::open(&f.0, StoragePolicy::default()).unwrap();
    let source = "def a():\n    return 1\n";
    let facts = cie_adapter_python::PythonAdapter.extract("app.py", source);
    assert!(writer
        .put_file(FileSubmission {
            path: "app.py",
            hash: [0; 32],
            source,
            facts: facts.clone(),
            tree: None
        })
        .is_err());
    assert!(writer
        .put_file(FileSubmission {
            path: "../app.py",
            hash: cie_core::types::content_hash(source.as_bytes()),
            source,
            facts,
            tree: None
        })
        .is_err());
    drop(writer);
    let mut invalid = StoragePolicy::default();
    invalid.cache_kib = 0;
    assert!(RepositoryWriter::open(&f.0, invalid).is_err());
    let mut overflow = StoragePolicy::default();
    overflow.max_page_size = usize::MAX;
    assert!(RepositoryWriter::open(&f.0, overflow).is_err());
}

#[test]
fn publication_lease_serializes_the_next_writer_until_head_publication_finishes() {
    use std::{sync::mpsc, time::Duration};
    let f = Fixture::new();
    let mut writer = RepositoryWriter::open(&f.0, StoragePolicy::default()).unwrap();
    submit(&mut writer, "app.py", "def a():\n    return 1\n");
    let graph = writer.materialize().unwrap();
    let publication = writer
        .commit(1, &graph, &serde_json::json!({}), &[])
        .unwrap();
    let root = f.0.clone();
    let (send, receive) = mpsc::channel();
    let next = std::thread::spawn(move || {
        let writer = RepositoryWriter::open(&root, StoragePolicy::default()).unwrap();
        send.send(()).unwrap();
        drop(writer);
    });
    assert!(receive.recv_timeout(Duration::from_millis(100)).is_err());
    drop(publication);
    receive.recv_timeout(Duration::from_secs(5)).unwrap();
    next.join().unwrap();
}
