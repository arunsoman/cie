//! On-demand sample repository (US-007): a small, realistic, dated history
//! any view can exercise end to end before connecting real code. Generation
//! is local and deterministic — fixed content, fixed commit dates, no
//! network call — so the same call always produces the same honest
//! material, never a claim about a real system.

use std::fs;
use std::path::Path;
use std::process::Command;

/// One generated commit: subject line + the relative paths it introduced.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SampleCommit {
    pub subject: String,
    pub files: Vec<String>,
}

/// What `generate_sample_repo` actually produced — every path and commit
/// named, so the demo is inspectable rather than a black box.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SampleRepoReport {
    pub root: String,
    pub commits: Vec<SampleCommit>,
    pub files_written: usize,
}

/// Honest failure: no sample was created, and nothing partial was left
/// claiming to be a complete demo.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SampleGenerationFailed {
    pub reason: String,
}

fn plan() -> Vec<(&'static str, Vec<(&'static str, &'static str)>, i64)> {
    // (subject, [(path, contents)], fixed commit unix time)
    vec![
        (
            "feat: scaffold the greeter service",
            vec![
                ("README.md", "# Greeter\n\nA tiny sample service used to exercise CIE end to end.\n"),
                (
                    "app/__init__.py",
                    "\"\"\"Sample greeter package (demo data, not real code).\"\"\"\n",
                ),
                (
                    "app/greeter.py",
                    "def format_greeting(name: str) -> str:\n    \"\"\"Return a friendly greeting for name.\"\"\"\n    return f\"Hello, {name}!\"\n",
                ),
            ],
            1_700_000_000,
        ),
        (
            "feat: add a service layer calling the greeter",
            vec![(
                "app/service.py",
                "from app.greeter import format_greeting\n\n\ndef greet_request(payload: dict) -> dict:\n    \"\"\"Build a greeting response from a request payload.\"\"\"\n    name = payload.get(\"name\", \"friend\")\n    return {\"message\": format_greeting(name)}\n",
            )],
            1_700_086_400,
        ),
        (
            "test: cover the greeter and service layer",
            vec![(
                "tests/test_service.py",
                "from app.service import greet_request\n\n\ndef test_greet_request_default():\n    assert greet_request({})[\"message\"] == \"Hello, friend!\"\n\n\ndef test_greet_request_named():\n    assert greet_request({\"name\": \"Ada\"})[\"message\"] == \"Hello, Ada!\"\n",
            )],
            1_700_172_800,
        ),
        (
            "fix: handle an empty name without crashing",
            vec![(
                "app/greeter.py",
                "def format_greeting(name: str) -> str:\n    \"\"\"Return a friendly greeting for name, defaulting when blank.\"\"\"\n    name = name.strip() if name else \"\"\n    return f\"Hello, {name or 'friend'}!\"\n",
            )],
            1_700_259_200,
        ),
    ]
}

fn run(root: &Path, args: &[&str]) -> Result<(), SampleGenerationFailed> {
    let out = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .map_err(|e| SampleGenerationFailed { reason: format!("git {:?} failed to start: {e}", args) })?;
    if !out.status.success() {
        return Err(SampleGenerationFailed {
            reason: format!("git {:?} exited {}: {}", args, out.status, String::from_utf8_lossy(&out.stderr)),
        });
    }
    Ok(())
}

/// Generate the sample app with dated history into `root`. Refuses to touch
/// a directory that already has content — a demo is never allowed to
/// overwrite real work.
pub fn generate_sample_repo(root: &Path) -> Result<SampleRepoReport, SampleGenerationFailed> {
    if root.exists() && fs::read_dir(root).map(|mut d| d.next().is_some()).unwrap_or(true) {
        return Err(SampleGenerationFailed {
            reason: format!("{} already exists and is not empty; refusing to overwrite", root.display()),
        });
    }
    fs::create_dir_all(root).map_err(|e| SampleGenerationFailed { reason: e.to_string() })?;
    run(root, &["init", "--quiet", "--initial-branch=main"])?;
    run(root, &["config", "user.email", "sample@example.invalid"])?;
    run(root, &["config", "user.name", "CIE Sample Generator"])?;

    let mut report_commits = Vec::new();
    let mut files_written = 0usize;
    for (subject, files, when) in plan() {
        let mut rel_paths = Vec::new();
        for (path, contents) in &files {
            let full = root.join(path);
            if let Some(parent) = full.parent() {
                fs::create_dir_all(parent).map_err(|e| SampleGenerationFailed { reason: e.to_string() })?;
            }
            fs::write(&full, contents).map_err(|e| SampleGenerationFailed { reason: e.to_string() })?;
            files_written += 1;
            rel_paths.push(path.to_string());
        }
        let mut add_args = vec!["add", "--"];
        add_args.extend(rel_paths.iter().map(|s| s.as_str()));
        run(root, &add_args)?;
        let date = format!("{when} +0000");
        let out = Command::new("git")
            .args(["commit", "--quiet", "-m", subject])
            .current_dir(root)
            .env("GIT_AUTHOR_DATE", &date)
            .env("GIT_COMMITTER_DATE", &date)
            .output()
            .map_err(|e| SampleGenerationFailed { reason: format!("git commit failed to start: {e}") })?;
        if !out.status.success() {
            return Err(SampleGenerationFailed {
                reason: format!("git commit exited {}: {}", out.status, String::from_utf8_lossy(&out.stderr)),
            });
        }
        report_commits.push(SampleCommit { subject: subject.to_string(), files: rel_paths });
    }

    Ok(SampleRepoReport {
        root: root.display().to_string(),
        commits: report_commits,
        files_written,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn git_available() -> bool {
        Command::new("git").arg("--version").output().map(|o| o.status.success()).unwrap_or(false)
    }

    #[test]
    fn generates_dated_history_with_named_files() {
        if !git_available() {
            return; // honest skip: no fabricated result without the git binary
        }
        let dir = std::env::temp_dir().join(format!("cie-sample-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let report = generate_sample_repo(&dir).expect("sample generation should succeed");
        assert_eq!(report.commits.len(), 4);
        assert!(report.files_written >= 4);
        assert!(dir.join("app/greeter.py").exists());
        let log = Command::new("git")
            .args(["log", "--format=%s"])
            .current_dir(&dir)
            .output()
            .unwrap();
        let subjects = String::from_utf8_lossy(&log.stdout);
        assert!(subjects.contains("scaffold the greeter service"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn refuses_to_overwrite_a_nonempty_directory() {
        let dir = std::env::temp_dir().join(format!("cie-sample-nonempty-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("keep.txt"), "do not touch").unwrap();
        let err = generate_sample_repo(&dir).unwrap_err();
        assert!(err.reason.contains("not empty"));
        assert!(dir.join("keep.txt").exists());
        let _ = fs::remove_dir_all(&dir);
    }
}
