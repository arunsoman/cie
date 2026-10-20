//! RFC-0019 stage 3: docker enrichment — compose `build:` captured, inline
//! port lists parsed, the governance join (compose / own-Dockerfile / none),
//! and image-only compose services surfaced as C4 Context externals.

use cie_stack::stack_report;
use std::fs;
use std::io::Write;
use std::path::PathBuf;

fn unique_dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "cie_rfc19_s3_{}_{}_{}",
        tag,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&d).unwrap();
    d
}

fn write(root: &PathBuf, rel: &str, content: &str) {
    let p = root.join(rel);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    let mut f = fs::File::create(p).unwrap();
    f.write_all(content.as_bytes()).unwrap();
}

/// the tracked polyglot fixture's compose uses inline-list ports and a
/// build context per service — the exact gaps RFC-0019 names
#[test]
fn polyglot_compose_reads_build_contexts_and_inline_ports() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../samples/polyglot");
    if !dir.exists() {
        eprintln!("fixture missing — skipped");
        return;
    }
    let r = stack_report(&dir);
    let compose = r.docker.compose.as_ref().expect("the fixture has a compose");
    assert_eq!(compose.services.len(), 6, "five build-governed services + the image-only cache");
    let berth = compose
        .services
        .iter()
        .find(|s| s.name == "berth-booking")
        .expect("service present");
    assert_eq!(berth.build.as_deref(), Some("./berth-booking"), "build context as written");
    assert_eq!(berth.ports, vec!["8081:8081"], "inline flow ports parsed");
    assert!(berth.line > 0, "the service key row is the evidence anchor");

    // the image-only service is NOT one of our apps — it is a C4 Context
    // external, named and cited as written
    let cache = r
        .external_docker
        .iter()
        .find(|e| e.name == "cache")
        .expect("the image-only service surfaces as an external");
    assert_eq!(cache.image.as_deref(), Some("redis:7-alpine"), "image as written");
    assert!(cache.ports.is_empty(), "no ports declared — nothing invented");
    assert!(cache.evidence.starts_with("docker-compose.yml:"), "cited file:row");

    // the governance join: every app dir matches a compose build context
    assert_eq!(r.apps.len(), 5);
    for app in &r.apps {
        let gov = app.docker.as_ref().expect("every app gets a governance record");
        assert_eq!(gov.kind, "compose", "{} is governed by compose", app.dir);
        assert_eq!(gov.service.as_deref(), Some(app.dir.as_str()));
        assert!(gov.evidence.starts_with("docker-compose.yml:"), "evidence cites file:row");
        assert_ne!(app.dir, "cache", "an image-only service never becomes an app");
    }
}

/// governance join arm 2: a Dockerfile under the app's dir, no compose
#[test]
fn own_dockerfile_governance() {
    let root = unique_dir("ownfile");
    write(&root, "svc-a/package.json", "{ \"name\": \"svc-a\", \"dependencies\": { \"express\": \"^4\" } }\n");
    write(&root, "svc-a/index.js", "const app = require('express')();\napp.get('/ping', (q, r) => r.send('pong'));\n");
    write(&root, "svc-a/Dockerfile", "FROM node:20\nEXPOSE 3000\n");
    write(&root, "svc-b/package.json", "{ \"name\": \"svc-b\" }\n");
    write(&root, "svc-b/main.js", "console.log('no http here');\n");
    let r = stack_report(&root);
    let a = r.apps.iter().find(|a| a.dir == "svc-a").expect("svc-a detected");
    let gov = a.docker.as_ref().expect("governance recorded");
    assert_eq!(gov.kind, "own-dockerfile", "the Dockerfile under svc-a governs it");
    assert_eq!(gov.evidence, "svc-a/Dockerfile");
    assert_eq!(gov.ports, vec!["3000"], "EXPOSE read as written");
    let b = r.apps.iter().find(|a| a.dir == "svc-b").expect("svc-b detected");
    let gov_b = b.docker.as_ref().expect("governance recorded");
    assert_eq!(gov_b.kind, "none");
    assert!(
        r.notes.iter().any(|n| n.contains("svc-b is not containerized as far as the repo says")),
        "the none-arm says so in plain words: {:?}",
        r.notes
    );
    fs::remove_dir_all(&root).ok();
}

/// governance join arm 3: compose names an app "none" when no row matches
/// and no Dockerfile sits under its dir; image-only services → externals
#[test]
fn compose_governance_externals_and_none() {
    let root = unique_dir("extern");
    // two real apps: one compose-governed, one not
    write(&root, "api/requirements.txt", "flask==3.0.0\n");
    write(&root, "api/app.py", "from flask import Flask\napp = Flask(__name__)\n@app.route('/hi')\ndef hi():\n    return 'hi'\n");
    write(&root, "worker/requirements.txt", "celery==5.3.0\n");
    write(&root, "worker/run.py", "print('worker')\n");
    // the compose: api builds from ./api; the database is image-only
    write(
        &root,
        "docker-compose.yml",
        "services:\n  api:\n    build: ./api\n    ports: [\"5000:5000\"]\n  postgres:\n    image: postgres:16\n    ports: [\"5432:5432\"]\n  legacy:\n    image: legacy/batch:v2\n",
    );
    let r = stack_report(&root);
    let api = r.apps.iter().find(|a| a.dir == "api").expect("api detected");
    assert_eq!(api.docker.as_ref().unwrap().kind, "compose");
    assert_eq!(api.docker.as_ref().unwrap().service.as_deref(), Some("api"));
    let worker = r.apps.iter().find(|a| a.dir == "worker").expect("worker detected");
    assert_eq!(
        worker.docker.as_ref().unwrap().kind,
        "none",
        "no compose row names worker and no Dockerfile sits under it"
    );
    let ext: Vec<_> = r.external_docker.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(ext, vec!["postgres", "legacy"], "image-only services are externals, in order");
    let pg = &r.external_docker[0];
    assert_eq!(pg.image.as_deref(), Some("postgres:16"), "image as written");
    assert_eq!(pg.ports, vec!["5432:5432"]);
    assert!(pg.evidence.starts_with("docker-compose.yml:"), "evidence cites file:row");
    fs::remove_dir_all(&root).ok();
}

/// malformed rows skip honestly, by name — nothing guessed
#[test]
fn malformed_compose_rows_skip_with_a_note() {
    let root = unique_dir("malformed");
    write(&root, "app/requirements.txt", "flask==3.0.0\n");
    write(&root, "app/main.py", "print('x')\n");
    write(
        &root,
        "docker-compose.yml",
        "services:\n  app:\n    build: ./app\n  ghost:\n    ports: [\"9000:9000\"]\n  app:\n    image: dup:1\n",
    );
    let r = stack_report(&root);
    let compose = r.docker.compose.as_ref().unwrap();
    assert_eq!(compose.services.len(), 1, "only the well-named service survives");
    assert_eq!(compose.services[0].name, "app");
    assert!(
        compose.skipped.contains(&"ghost".to_string()),
        "a service naming neither image nor build is skipped by name"
    );
    assert!(
        compose.skipped.contains(&"app".to_string()) || r.notes.iter().any(|n| n.contains("`app`")),
        "the duplicate key is named, not silently dropped"
    );
    assert!(
        r.notes.iter().any(|n| n.contains("compose reader skipped")),
        "the skip surfaces as a note"
    );
    fs::remove_dir_all(&root).ok();
}
