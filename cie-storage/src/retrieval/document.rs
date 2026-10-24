//! Immutable captured JSON, decoded one top-level field at a time. Opening a
//! reader never constructs the repository's symbol/summary object graph.
use super::*;
use serde_json::value::RawValue;
use std::sync::OnceLock;
#[derive(Default)]
pub(crate) struct PinnedDocument {
    fields: BTreeMap<String, (Box<RawValue>, OnceLock<std::result::Result<Value, String>>)>,
}
static NULL: Value = Value::Null;
impl PinnedDocument {
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        let raw: BTreeMap<String, Box<RawValue>> = serde_json::from_slice(bytes)
            .map_err(|e| StorageError::new("CorruptData", e.to_string()))?;
        Ok(Self {
            fields: raw
                .into_iter()
                .map(|(key, value)| (key, (value, OnceLock::new())))
                .collect(),
        })
    }
    pub fn get(&self, key: &str) -> Option<&Value> {
        let (raw, parsed) = self.fields.get(key)?;
        parsed
            .get_or_init(|| serde_json::from_str(raw.get()).map_err(|e| e.to_string()))
            .as_ref()
            .ok()
    }
    pub fn to_value(&self) -> Value {
        Value::Object(
            self.fields
                .keys()
                .map(|key| (key.clone(), self.get(key).cloned().unwrap_or(Value::Null)))
                .collect(),
        )
    }
    pub fn check(&self) -> Result<()> {
        for (key, (_, parsed)) in &self.fields {
            if let Some(Err(error)) = parsed.get() {
                return Err(StorageError::new(
                    "CorruptData",
                    format!("Captured {key}: {error}"),
                ));
            }
        }
        Ok(())
    }
    pub fn keys(&self) -> Vec<&str> {
        self.fields
            .iter()
            .filter(|(_, (raw, _))| raw.get().trim() != "null")
            .map(|(key, _)| key.as_str())
            .collect()
    }
    pub fn decoded_fields(&self) -> Vec<&str> {
        self.fields
            .iter()
            .filter(|(_, (_, parsed))| parsed.get().is_some())
            .map(|(key, _)| key.as_str())
            .collect()
    }
}
impl std::ops::Index<&str> for PinnedDocument {
    type Output = Value;
    fn index(&self, key: &str) -> &Value {
        self.get(key).unwrap_or(&NULL)
    }
}
