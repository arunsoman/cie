//! Called from a Node worker thread, never the application event loop.
use cie_storage::{retrieval::*, StoragePolicy};
use napi_derive::napi;
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};
struct Context {
    reader: RevisionReader,
    touched: Instant,
}
static READERS: OnceLock<Mutex<BTreeMap<String, Context>>> = OnceLock::new();
static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
fn dispatch(input: &str) -> Result<Value> {
    if input.len() > 1024 * 1024 {
        return Err(StorageError::new("InvalidRequest", "Request exceeds 1 MiB"));
    }
    let v: Value = serde_json::from_str(input)?;
    let mut readers = READERS
        .get_or_init(Default::default)
        .lock()
        .map_err(|_| StorageError::new("Io", "Reader registry poisoned"))?;
    readers.retain(|_, c| c.touched.elapsed() < Duration::from_secs(900));
    let op = v["op"].as_str().unwrap_or("");
    // Revision listing and resolution take no context handle.
    if op == "retain" {
        let root = v["root"]
            .as_str()
            .ok_or_else(|| StorageError::new("InvalidRequest", "Missing root"))?;
        let destination = v["destination"]
            .as_str()
            .ok_or_else(|| StorageError::new("InvalidRequest", "Missing destination"))?;
        Storage::open(std::path::Path::new(root), StoragePolicy::default())?
            .retain_to(std::path::Path::new(destination))?;
        return Ok(json!({"retained":true}));
    }
    if op == "revisions" {
        let root = v["root"].as_str().ok_or_else(|| {
            StorageError::new("InvalidRequest", "Host must supply repository root")
        })?;
        let limit = v["limit"].as_u64().unwrap_or(100) as usize;
        let after = v["after"].as_str().map(str::to_string);
        return Ok(serde_json::to_value(
            Storage::open(std::path::Path::new(root), StoragePolicy::default())?
                .list_revisions(RevisionListRequest { limit, after })?,
        )?);
    }
    if op == "open" {
        if readers.len() >= 32 {
            return Err(StorageError::new(
                "BudgetExceeded",
                "Too many open revision contexts",
            ));
        }
        let root = v["root"].as_str().ok_or_else(|| {
            StorageError::new("InvalidRequest", "Host must supply repository root")
        })?;
        let selector = if let Some(rev) = v["revision"].as_str() {
            RevisionSelector::Id(rev.into())
        } else {
            RevisionSelector::Latest
        };
        let reader = Storage::open(std::path::Path::new(root), StoragePolicy::default())?
            .open_revision(selector)?;
        let id = format!(
            "ctx_{}_{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        );
        let capabilities = reader.capabilities();
        readers.insert(
            id.clone(),
            Context {
                reader,
                touched: Instant::now(),
            },
        );
        return Ok(json!({"context":id,"capabilities":capabilities}));
    }
    let id = v["context"]
        .as_str()
        .ok_or_else(|| StorageError::new("InvalidRequest", "Missing context"))?;
    if op == "close" {
        return Ok(json!({"closed":readers.remove(id).is_some()}));
    }
    let context = readers
        .get_mut(id)
        .ok_or_else(|| StorageError::new("CursorExpired", "Revision context expired or closed"))?;
    context.touched = Instant::now();
    match op {
        "describe" => Ok(tools::describe(&context.reader)),
        "retrieve" => {
            Ok(serde_json::to_value(context.reader.retrieve(
                serde_json::from_value(v["request"].clone())?,
            )?)?)
        }
        "tool_retrieve" => Ok(serde_json::to_value(tools::retrieve(
            &context.reader,
            &v["request"].to_string(),
        )?)?),
        "resolve" => {
            Ok(serde_json::to_value(context.reader.resolve_symbol(
                serde_json::from_value(v["lookup"].clone())?,
            )?)?)
        }
        "capabilities" => Ok(context.reader.capabilities()),
        "compatibility" => context
            .reader
            .ui_compatibility(v["kind"].as_str().unwrap_or(""), &v["args"]),
        "diagnostics" => context.reader.diagnostics(
            v["dataset"].as_str(),
            v["limit"].as_u64().unwrap_or(100) as usize,
            v["offset"].as_u64().unwrap_or(0) as usize,
        ),
        _ => Err(StorageError::new("InvalidRequest", "Unknown operation")),
    }
}
#[napi]
pub fn storage_dispatch(input: String) -> String {
    let result = std::panic::catch_unwind(|| dispatch(&input));
    let response = match result {
        Ok(Ok(value)) => json!({"ok":true,"result":value}),
        Ok(Err(error)) => json!({"ok":false,"error":error}),
        Err(_) => json!({"ok":false,"error":{"code":"Io","message":"Storage worker failed"}}),
    };
    response.to_string()
}
