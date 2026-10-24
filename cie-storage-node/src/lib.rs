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
static CANCELLATIONS: OnceLock<
    Mutex<BTreeMap<String, std::sync::Arc<std::sync::atomic::AtomicBool>>>,
> = OnceLock::new();
#[napi]
pub fn storage_cancel(id: String) -> bool {
    let mut tokens = CANCELLATIONS.get_or_init(Default::default).lock().unwrap();
    if tokens.len() >= 1024 {
        tokens.retain(|_, token| std::sync::Arc::strong_count(token) > 1);
    }
    if let Some(token) = tokens.get(&id) {
        token.store(true, std::sync::atomic::Ordering::Relaxed);
        true
    } else if tokens.len() < 1024 {
        tokens.insert(
            id,
            std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true)),
        );
        true
    } else {
        false
    }
}
static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
fn dispatch(input: &str) -> Result<Value> {
    if input.len() > 1024 * 1024 {
        return Err(StorageError::new("InvalidRequest", "Request exceeds 1 MiB"));
    }
    let v: Value = serde_json::from_str(input)?;
    let cancellation_id = v["cancellation_id"].as_str().unwrap_or("").to_owned();
    let token = {
        let mut tokens = CANCELLATIONS.get_or_init(Default::default).lock().unwrap();
        if tokens.len() >= 1024 && !tokens.contains_key(&cancellation_id) {
            return Err(StorageError::new(
                "BudgetExceeded",
                "Too many pending requests",
            ));
        }
        tokens.entry(cancellation_id.clone()).or_default().clone()
    };
    struct Cleanup(String);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            if let Ok(mut tokens) = CANCELLATIONS.get().unwrap().lock() {
                tokens.remove(&self.0);
            }
        }
    }
    let _cleanup = Cleanup(cancellation_id);
    if token.load(std::sync::atomic::Ordering::Relaxed) {
        return Err(StorageError::new("Cancelled", "Request cancelled"));
    }
    let mut readers = READERS
        .get_or_init(Default::default)
        .lock()
        .map_err(|_| StorageError::new("Io", "Reader registry poisoned"))?;
    readers.retain(|_, c| c.touched.elapsed() < Duration::from_secs(900));
    let op = v["op"].as_str().unwrap_or("");
    let requested_context = v["request"]["context"].as_str();
    let effective_context = v["context"].as_str().or(requested_context);
    if v["context"]
        .as_str()
        .zip(requested_context)
        .is_some_and(|(a, b)| a != b)
    {
        return Err(StorageError::new(
            "MixedRevisionReference",
            "Context handles do not match",
        ));
    }
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
    if op == "open" || (op == "describe" && effective_context.is_none()) {
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
        if op == "describe" {
            let mut description = tools::describe_with_options(
                &readers[&id].reader,
                if v["request"].is_null() {
                    Default::default()
                } else {
                    serde_json::from_value(v["request"].clone())?
                },
            )?;
            description["context"] = json!(id);
            return Ok(description);
        }
        return Ok(json!({"context":id,"capabilities":capabilities}));
    }
    let id =
        effective_context.ok_or_else(|| StorageError::new("InvalidRequest", "Missing context"))?;
    if op == "close" {
        return Ok(json!({"closed":readers.remove(id).is_some()}));
    }
    if op == "tool_retrieve" {
        if let Some(requested) = v["request"]["context"].as_str() {
            if requested != id {
                return Err(StorageError::new(
                    "MixedRevisionReference",
                    "Tool context does not match the authorized reader",
                ));
            }
        }
    }
    let context = readers
        .get_mut(id)
        .ok_or_else(|| StorageError::new("CursorExpired", "Revision context expired or closed"))?;
    context.touched = Instant::now();
    context.reader.run_bounded(token.clone(), || match op {
        "describe" => {
            let mut description = tools::describe_with_options(
                &context.reader,
                if v["request"].is_null() {
                    Default::default()
                } else {
                    serde_json::from_value(v["request"].clone())?
                },
            )?;
            description["context"] = json!(id);
            Ok(description)
        }
        "retrieve" => Ok(serde_json::to_value(context.reader.retrieve_with_cancel(
            serde_json::from_value(v["request"].clone())?,
            token,
        )?)?),
        "tool_retrieve" => Ok(serde_json::to_value(tools::retrieve_with_cancel(
            &context.reader,
            &v["request"].to_string(),
            token,
        )?)?),
        "explain" => Ok(context
            .reader
            .explain(&serde_json::from_value(v["request"].clone())?)),
        "resolve" => {
            Ok(serde_json::to_value(context.reader.resolve_symbol(
                serde_json::from_value(v["lookup"].clone())?,
            )?)?)
        }
        "capabilities" => Ok(context.reader.capabilities()),
        "diagnostics" => context.reader.diagnostics(
            v["dataset"].as_str(),
            v["limit"].as_u64().unwrap_or(100) as usize,
            v["offset"].as_u64().unwrap_or(0) as usize,
        ),
        _ => Err(StorageError::new("InvalidRequest", "Unknown operation")),
    })
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
