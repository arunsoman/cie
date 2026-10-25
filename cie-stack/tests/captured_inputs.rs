//! RFC-0019 §6.2 handoff: ingest's read phase already reads every
//! stack-input file to hash it — [`cie_stack::stack_report_with`] feeds the
//! lens those captured bodies so no stack-input file is opened twice in one
//! ingest. These tests prove the capture is genuinely consulted (the files
//! can be GONE from disk and the lens still reads them) and that the report
//! is byte-identical with or without the capture.
use cie_stack::{is_stack_input, stack_report, stack_report_with};
use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

fn unique_dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "cie_stack_capture_{}_{}_{}",
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

/// one api/ app + docker + config — every stack-input file a walk would
/// capture, plus one code file the lens reads from disk as before
fn fixture(root: &PathBuf) {
    write(
        root,
        "api/pom.xml",
        r#"<?xml version="1.0"?>
<project xmlns="http://maven.apache.org/POM/4.0.0">
  <modelVersion>4.0.0</modelVersion>
  <groupId>demo</groupId>
  <artifactId>demo-api</artifactId>
  <version>1.0.0</version>
  <parent>
    <groupId>org.springframework.boot</groupId>
    <artifactId>spring-boot-starter-parent</artifactId>
    <version>3.2.0</version>
  </parent>
  <dependencies>
    <dependency>
      <groupId>org.springframework.boot</groupId>
      <artifactId>spring-boot-starter-web</artifactId>
    </dependency>
  </dependencies>
</project>
"#,
    );
    write(
        root,
        "api/src/main/java/demo/HiController.java",
        r#"package demo;

import org.springframework.web.bind.annotation.GetMapping;
import org.springframework.web.bind.annotation.RestController;

@RestController
public class HiController {
    @GetMapping("/hi")
    public String hi() {
        return "hi";
    }
}
"#,
    );
    write(
        root,
        "docker-compose.yml",
        "services:\n  api:\n    build: ./api\n    ports: [\"8080:8080\"]\n",
    );
    write(root, "api/Dockerfile", "FROM eclipse-temurin:21\nEXPOSE 8080\n");
    write(
        root,
        "api/src/main/resources/application.properties",
        "server.port=8080\nspring.profiles.active=dev\n",
    );
}

/// capture like ingest's read phase (stream.rs) does: every stack-input
/// file's utf8 body, keyed by the walk's normalization (rel, forward slashes)
fn capture_stack_inputs(root: &Path) -> BTreeMap<String, String> {
    let mut bodies = BTreeMap::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for e in fs::read_dir(&dir).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
                continue;
            }
            let rel = p
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            if is_stack_input(&rel) {
                bodies.insert(rel, fs::read_to_string(&p).unwrap());
            }
        }
    }
    bodies
}

/// the capture is really read: with the stack-input file deleted from disk,
/// `stack_report_with` still reports the manifest-grounded facts — while the
/// disk-only lens honestly reports nothing. (Root-manifest reads are
/// name-keyed, not walk-enumerated, so a captured root pom works with the
/// file gone; docker/compose/config reads are gated on what the disk walk
/// still enumerates, so they belong to the stale-copy test below.)
#[test]
fn captured_bodies_feed_the_lens_with_the_files_gone() {
    let root = unique_dir("gone");
    write(
        &root,
        "pom.xml",
        r#"<?xml version="1.0"?>
<project xmlns="http://maven.apache.org/POM/4.0.0">
  <modelVersion>4.0.0</modelVersion>
  <artifactId>demo-api</artifactId>
  <parent>
    <groupId>org.springframework.boot</groupId>
    <artifactId>spring-boot-starter-parent</artifactId>
    <version>3.2.0</version>
  </parent>
  <dependencies>
    <dependency>
      <groupId>org.springframework.boot</groupId>
      <artifactId>spring-boot-starter-web</artifactId>
    </dependency>
  </dependencies>
</project>
"#,
    );
    let mut bodies = BTreeMap::new();
    bodies.insert(
        "pom.xml".to_string(),
        fs::read_to_string(root.join("pom.xml")).unwrap(),
    );

    // the handoff under test: delete the captured file, then run the lens
    fs::remove_file(root.join("pom.xml")).unwrap();

    let r = stack_report_with(&root, Some(&bodies));
    assert_eq!(r.build.tool, "maven", "the captured pom fed the detector");
    let fw = r
        .frameworks
        .iter()
        .find(|f| f.id == "spring-boot")
        .expect("the captured pom grounds the framework claim");
    assert_eq!(fw.name, "Spring Boot 3.2.0");
    assert_eq!(fw.modules, vec!["spring-boot-starter-parent", "spring-boot-starter-web"]);

    // the control: without the capture those reads honestly fail
    let disk_only = stack_report(&root);
    assert_eq!(disk_only.build.tool, "unknown");
    assert!(disk_only.frameworks.is_empty());

    fs::remove_dir_all(&root).ok();
}

/// the capture, not disk, is the lens' source of truth for stack inputs:
/// rewrite every captured file with junk AFTER capturing — the report must
/// still describe what the walk recorded (the same bytes its §6.2 hash
/// covers), while the disk-only lens reads the junk as written
#[test]
fn captured_bodies_win_over_disk_after_the_files_change() {
    let root = unique_dir("stale");
    fixture(&root);
    let bodies = capture_stack_inputs(&root);

    // yesterday's junk lands where yesterday's manifests were
    write(&root, "api/pom.xml", "not really a pom\n");
    write(
        &root,
        "docker-compose.yml",
        "services:\n  other:\n    image: busybox:1\n",
    );
    write(&root, "api/Dockerfile", "FROM scratch\n");
    write(&root, "api/src/main/resources/application.properties", "mail.host=relay\n");

    let r = stack_report_with(&root, Some(&bodies));
    assert!(
        r.frameworks
            .iter()
            .any(|f| f.id == "spring" || f.id == "spring-boot"),
        "the captured pom grounds the spring claim"
    );
    assert!(
        r.routes.iter().any(|rt| rt.path == "/hi" && rt.service == "api"),
        "routes ran off the captured pom + the on-disk java source"
    );
    let compose = r.docker.compose.as_ref().expect("captured compose parsed");
    assert!(compose.services.iter().any(|s| s.name == "api"), "the captured compose, not the junk one");
    assert!(
        r.docker
            .dockerfiles
            .iter()
            .any(|d| d.base_image.as_deref() == Some("eclipse-temurin:21")),
        "captured Dockerfile read as written"
    );
    assert!(
        r.config
            .iter()
            .any(|c| c.rel == "api/src/main/resources/application.properties" && c.port == Some(8080)),
        "captured config parsed"
    );

    // the control: the same lens on disk reads the junk as written
    let disk_only = stack_report(&root);
    assert!(
        !disk_only
            .frameworks
            .iter()
            .any(|f| f.id == "spring" || f.id == "spring-boot"),
        "junk pom claims no spring"
    );
    assert!(disk_only.routes.is_empty(), "no spring app detected — no routes");
    let junk_compose = disk_only.docker.compose.as_ref().expect("junk compose parsed");
    assert!(junk_compose.services.iter().any(|s| s.name == "other"));
    assert!(!junk_compose.services.iter().any(|s| s.name == "api"));
    assert!(!disk_only.config.iter().any(|c| c.port == Some(8080)));

    fs::remove_dir_all(&root).ok();
}

/// identical reports with or without the capture — the handoff changes the
/// I/O, never the output
#[test]
fn captured_bodies_report_is_identical_to_the_disk_report() {
    let root = unique_dir("parity");
    fixture(&root);

    let mut a = serde_json::to_value(stack_report(&root)).unwrap();
    let b = serde_json::to_value(stack_report_with(&root, Some(&capture_stack_inputs(&root)))).unwrap();
    // only `generated_at` may differ — zero it on both sides
    a["generatedAt"] = serde_json::json!(0);
    let mut b = b;
    b["generatedAt"] = serde_json::json!(0);
    assert_eq!(a, b, "same bytes read, same report");

    fs::remove_dir_all(&root).ok();
}

/// the capture only replaces reads of files the walk would capture — the
/// lens' code-file reads must stay disk reads even if a caller stuffs a
/// body for one into the map
#[test]
fn capture_cannot_impersonate_files_the_walk_does_not_watch() {
    let root = unique_dir("sneaky");
    fixture(&root);
    let mut bodies = capture_stack_inputs(&root);
    // a java file is a SOURCE, not a stack input — no walk captures it
    assert!(!is_stack_input("src/main/java/demo/HiController.java"));
    bodies.insert(
        "src/main/java/demo/HiController.java".into(),
        "package demo; class Broken {} // sabotaged body\n".into(),
    );

    let r = stack_report_with(&root, Some(&bodies));
    assert!(
        r.routes.iter().any(|rt| rt.path == "/hi"),
        "the genuine controller from disk won — the sabotaged body never reached the parser"
    );

    fs::remove_dir_all(&root).ok();
}
