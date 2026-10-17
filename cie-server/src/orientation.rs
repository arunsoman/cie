//! Repository orientation operates only on declared roots and immutable snapshots.
use crate::{api::App, model::*, repository};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{collections::BTreeSet, fs, path::Path, process::Command, sync::Arc};
fn git(root: &Path, args: &[&str]) -> Result<Vec<u8>> {
    let output = Command::new("git")
        .args([
            "-c",
            "core.fsmonitor=false",
            "-c",
            "log.showSignature=false",
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "init.templateDir=",
        ])
        .arg("-C")
        .arg(root)
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()?;
    if !output.status.success() {
        return Err(Error::new(
            400,
            "GIT_UNAVAILABLE",
            "Git operation failed; no source changes were applied",
        ));
    }
    Ok(output.stdout)
}
fn parse<T: serde::de::DeserializeOwned>(body: &[u8]) -> Result<T> {
    serde_json::from_slice(body).map_err(|_| {
        Error::new(
            400,
            "INVALID_INPUT",
            "Request does not match supported orientation schema",
        )
    })
}
fn sample(app: &Arc<App>) -> Result<Value> {
    let root = app.storage.directory.join("samples").join("checkout");
    // This endpoint owns only its private generated directory; never initializes
    // or changes a user-supplied repository.
    let repo=app.storage.transaction(|s|{
        if let Some(r)=s.repositories.iter().find(|r|Path::new(&r.path)==root){return Ok(r.clone());}
        if root.exists(){return Err(Error::new(409,"SAMPLE_EXISTS","An unconnected sample directory already exists. Inspect it before retrying; it was not overwritten."));}
        if s.repositories.len()>=32{return Err(Error::new(409,"REPOSITORY_QUOTA","Repository limit reached"));}
        fs::create_dir_all(&root)?;
        let create=(||->Result<()>{
            fs::write(root.join("README.md"),"# Synthetic checkout sample\n\nGenerated evaluation fixture, not production evidence. Its Git history uses explicit synthetic dates. The second revision intentionally introduces a misleading authorize implementation. No real payment or inventory operation occurs. The regression also weakens the test expectation; passing that test would not independently establish the intended authorization behavior.\n")?;
            fs::write(root.join("checkout.py"),"def authorize(amount):\n    if amount <= 0:\n        raise ValueError('Amount must be positive')\n    return {'authorized': True, 'amount': amount}\n\ndef checkout(amount):\n    return authorize(amount)\n\ndef test_checkout():\n    assert checkout(10)['authorized']\n")?;
            fs::create_dir_all(root.join("domain"))?; fs::create_dir_all(root.join("web"))?; fs::create_dir_all(root.join("gateway"))?;
            fs::write(root.join("domain/cart.py"),"class Cart:\n    def __init__(self):\n        self.items = []\n\n    def add_item(self, price, quantity):\n        if quantity <= 0:\n            raise ValueError('Quantity must be positive')\n        self.items.append((price, quantity))\n\n    def total(self):\n        return sum(price * quantity for price, quantity in self.items)\n")?;
            fs::write(root.join("web/checkout.ts"),"export interface CheckoutRequest { amount: number; }\nexport async function submitCheckout(request: CheckoutRequest) {\n  return fetch('/checkout', { method: 'POST', body: JSON.stringify(request) });\n}\n")?;
            fs::write(root.join("web/format.js"),"export function formatAmount(amount) { return amount.toFixed(2); }\n")?;
            fs::write(root.join("gateway/PaymentGateway.java"),"package gateway;\npublic interface PaymentGateway { boolean authorize(long amount); }\n")?;
            fs::write(root.join("unsupported.kotlin"),"// Intentionally unsupported fixture to exercise coverage gaps.\n")?;
            git(&root,&["init","--initial-branch=main"])?;git(&root,&["add","."])?;
            for (date,message,changed) in [("2025-01-10T10:00:00+00:00","Synthetic baseline checkout",false),("2025-02-10T10:00:00+00:00","Synthetic misleading authorization regression",true)] {
                if changed{fs::write(root.join("checkout.py"),"def authorize(amount):\n    # Intentionally misleading fixture: no authorization occurs.\n    print('hello world')\n\ndef checkout(amount):\n    return authorize(amount)\n\ndef test_checkout():\n    assert checkout(10) is None\n")?;git(&root,&["add","checkout.py"])?;}
                let output=Command::new("git").args(["-c","core.hooksPath=/dev/null","-c","core.fsmonitor=false", "-c", "log.showSignature=false","-c","user.name=CIE synthetic fixture","-c","user.email=fixture@localhost"]).arg("-C").arg(&root).args(["commit","-m",message]).env("GIT_CONFIG_NOSYSTEM","1").env("GIT_CONFIG_GLOBAL","/dev/null").env("GIT_AUTHOR_DATE",date).env("GIT_COMMITTER_DATE",date).output()?;
                if !output.status.success(){return Err(Error::new(400,"SAMPLE_FAILED","Could not create synthetic Git history"));}
            }Ok(())})();
        if let Err(e)=create{let _=fs::remove_dir_all(&root);return Err(e);}
        let r=Repository{id:s.id("repo"),name:"Synthetic checkout sample".into(),path:root.to_string_lossy().into(),revisions:vec![]};s.repositories.push(r.clone());Ok(r)
    })?;
    Ok(
        json!({"repository":repo,"synthetic":true,"gaps":["This deliberately defective fixture is synthetic, not expert or production evidence.","Index explicitly before asking questions. Unsupported views remain unavailable."]}),
    )
}
pub fn dispatch(app: &Arc<App>, method: &str, path: &str, body: &[u8]) -> Option<Result<Value>> {
    let p: Vec<_> = path.trim_matches('/').split('/').collect();
    let supported = matches!(
        (method, p.as_slice()),
        ("POST", ["api", "samples"])
            | ("GET", ["api", "repositories", _, "history"])
            | ("POST", ["api", "repositories", _, "compare"])
            | ("POST", ["api", "source"])
    );
    if !supported {
        return None;
    }
    Some((|| match (method, p.as_slice()) {
        ("POST", ["api", "samples"]) => {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct New {
                kind: String,
            }
            let req: New = parse(body)?;
            if req.kind != "checkout" {
                return Err(Error::new(
                    400,
                    "UNKNOWN_SAMPLE",
                    "Only checkout is available",
                ));
            }
            sample(app)
        }
        ("GET", ["api", "repositories", id, "history"]) => {
            let s = app.storage.read()?;
            let r = s
                .repositories
                .iter()
                .find(|r| r.id == *id)
                .ok_or_else(|| Error::new(404, "NOT_FOUND", "Repository unavailable"))?;
            let root = repository::allowed_path(Path::new(&r.path), &app.allowed_roots)?;
            let output = git(&root, &["log", "-n", "50", "--format=%H %ct"])?;
            let text = String::from_utf8_lossy(&output);
            let mut commits = vec![];
            for line in text.lines() {
                let fields: Vec<_> = line.split_whitespace().collect();
                if fields.len() != 2
                    || ![40, 64].contains(&fields[0].len())
                    || !fields[0].bytes().all(|b| b.is_ascii_hexdigit())
                {
                    continue;
                }
                let timestamp = fields[1]
                    .parse::<u64>()
                    .map_err(|_| Error::new(400, "GIT_HISTORY", "Invalid Git timestamp"))?;
                let names = git(
                    &root,
                    &[
                        "diff-tree",
                        "--no-commit-id",
                        "--name-only",
                        "-r",
                        "--root",
                        "-z",
                        fields[0],
                    ],
                )?;
                let files: Vec<_> = names
                    .split(|b| *b == 0)
                    .filter(|s| !s.is_empty())
                    .take(501)
                    .map(|s| String::from_utf8_lossy(s).to_string())
                    .collect();
                commits.push(json!({"commit":fields[0],"timestampSeconds":timestamp,"files":files.iter().take(500).map(|s|crate::answers::secret_safe(s)).collect::<Vec<_>>(),"filesCapped":files.len()>500,"claimClass":"gitMetadata"}));
            }
            Ok(
                json!({"repositoryId":r.id,"commits":commits,"limit":50,"possiblyCapped":commits.len()==50,"authorshipCollected":false,"gaps":["Commit times are repository-supplied metadata, not independently verified event times.","Co-change does not prove dependency or causality. Merge-parent coverage is incomplete."]}),
            )
        }
        ("POST", ["api", "repositories", id, "compare"]) => {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct Compare {
                before: String,
                after: String,
            }
            let req: Compare = parse(body)?;
            let s = app.storage.read()?;
            let r = s
                .repositories
                .iter()
                .find(|r| r.id == *id)
                .ok_or_else(|| Error::new(404, "NOT_FOUND", "Repository unavailable"))?;
            let before = r
                .revisions
                .iter()
                .find(|v| v.id == req.before)
                .ok_or_else(|| Error::new(410, "EVIDENCE_GONE", "Before snapshot unavailable"))?;
            let after = r
                .revisions
                .iter()
                .find(|v| v.id == req.after)
                .ok_or_else(|| Error::new(410, "EVIDENCE_GONE", "After snapshot unavailable"))?;
            let files: BTreeSet<_> = before.sources.keys().chain(after.sources.keys()).collect();
            let differences:Vec<_>=files.into_iter().filter_map(|file|{let a=before.sources.get(file);let b=after.sources.get(file);if a==b{return None;}let status=if a.is_none(){"added"}else if b.is_none(){"removed"}else{"changed"};Some(json!({"file":file,"status":status,"beforeLines":a.map(|s|s.lines().count()),"afterLines":b.map(|s|s.lines().count()),"claimClass":"codeObservation"}))}).collect();
            Ok(
                json!({"repositoryId":r.id,"before":before.id,"after":after.id,"differences":differences,"beforeFreshness":repository::freshness(r,before),"afterFreshness":repository::freshness(r,after),"gaps":["Comparison covers retained supported source text, not every repository file or behavior.","Text differences do not establish semantic compatibility, causal impact or validation success."]}),
            )
        }
        ("POST", ["api", "source"]) => {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase", deny_unknown_fields)]
            struct Source {
                repository_id: String,
                revision_id: String,
                file: String,
            }
            let req: Source = parse(body)?;
            let s = app.storage.read()?;
            let r = s
                .repositories
                .iter()
                .find(|r| r.id == req.repository_id)
                .ok_or_else(|| Error::new(404, "NOT_FOUND", "Repository unavailable"))?;
            let rev = r
                .revisions
                .iter()
                .find(|v| v.id == req.revision_id)
                .ok_or_else(|| {
                    Error::new(410, "EVIDENCE_GONE", "Exact source snapshot unavailable")
                })?;
            let source = rev.sources.get(&req.file).ok_or_else(|| {
                Error::new(
                    404,
                    "SOURCE_UNAVAILABLE",
                    "Path is not in this exact supported source snapshot",
                )
            })?;
            let safe = crate::answers::secret_safe(source);
            let truncated = safe.chars().count() > 200_000;
            let display: String = safe.chars().take(200_000).collect();
            Ok(
                json!({"repositoryId":r.id,"revisionId":rev.id,"file":req.file,"sourceHash":cie_core::types::hash_hex(&cie_core::types::content_hash(source.as_bytes())),"redactionMayChangeDisplayedText":true,"truncated":truncated,"text":display,"freshness":repository::freshness(r,rev)}),
            )
        }
        _ => Err(Error::new(
            404,
            "NOT_FOUND",
            "Orientation operation unavailable",
        )),
    })())
}
