use crate::model::*;
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::Mutex,
};
pub struct Storage {
    pub directory: PathBuf,
    pub state: Mutex<State>,
    _lock: std::fs::File,
}
impl Storage {
    pub fn open(directory: &Path) -> Result<Self> {
        fs::create_dir_all(directory)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(directory, fs::Permissions::from_mode(0o700))?;
        }
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(directory.join("workspace.lock"))?;
        fs2::FileExt::try_lock_exclusive(&lock).map_err(|_| {
            Error::new(
                409,
                "WORKSPACE_IN_USE",
                "Another process already owns this workspace",
            )
        })?;
        let file = directory.join("workspace.json");
        let mut state: State = if file.exists() {
            let metadata = fs::metadata(&file)?;
            if metadata.len() > 64 * 1024 * 1024 {
                return Err(Error::new(
                    500,
                    "STATE_LIMIT",
                    "Workspace state exceeds its size limit",
                ));
            }
            serde_json::from_slice(&fs::read(&file)?).map_err(|_|Error::new(500,"CORRUPT_STATE","Workspace state is corrupt; restore a verified backup instead of silently resetting it"))?
        } else {
            State::default()
        };
        if state.schema != 1 {
            return Err(Error::new(
                500,
                "INCOMPATIBLE_STATE",
                "Unsupported workspace schema",
            ));
        }
        for plan in &mut state.plans {
            if ["queued", "running", "cancelling"].contains(&plan.status.as_str()) {
                plan.status = "interrupted".into();
                for step in &mut plan.steps {
                    if ["queued", "running"].contains(&step.status.as_str()) {
                        step.status = "interrupted".into();
                        step.issue =
                            Some("Interrupted; explicitly resume after freshness checks".into());
                    }
                }
            }
        }
        for job in &mut state.jobs {
            if job.active() {
                job.state = "failed".into();
                job.stage = "interrupted".into();
                job.issue = Some(
                    "Interrupted by restart. No new analysis was published; reindex explicitly."
                        .into(),
                );
            }
        }
        let storage = Self {
            directory: std::fs::canonicalize(directory)?,
            state: Mutex::new(state),
            _lock: lock,
        };
        storage.transaction(|_| Ok(()))?;
        Ok(storage)
    }
    pub fn read(&self) -> Result<State> {
        Ok(self
            .state
            .lock()
            .map_err(|_| Error::new(500, "STATE_LOCK", "Workspace unavailable"))?
            .clone())
    }
    pub fn transaction<T>(&self, f: impl FnOnce(&mut State) -> Result<T>) -> Result<T> {
        let mut guard = self
            .state
            .lock()
            .map_err(|_| Error::new(500, "STATE_LOCK", "Workspace unavailable"))?;
        let mut next = guard.clone();
        let result = f(&mut next)?;
        let bytes = serde_json::to_vec(&next)
            .map_err(|_| Error::new(500, "SERIALIZE", "Cannot serialize workspace"))?;
        if bytes.len() > 64 * 1024 * 1024 {
            return Err(Error::new(
                409,
                "WORKSPACE_QUOTA",
                "Workspace is full; delete unused repositories or answers",
            ));
        }
        let temporary = self.directory.join("workspace.json.pending");
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        // Only this process owns storage; an orphan pending file is never authoritative.
        if temporary.exists() {
            fs::remove_file(&temporary)?;
        }
        let mut file = options.open(&temporary)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        fs::rename(&temporary, self.directory.join("workspace.json"))?;
        std::fs::File::open(&self.directory)?.sync_all()?;
        *guard = next;
        Ok(result)
    }
}

impl Drop for Storage {
    fn drop(&mut self) {
        // A concurrent fork can briefly inherit this open file description before
        // exec closes CLOEXEC descriptors. Explicit unlock releases ownership
        // immediately instead of waiting for every inherited reference to close.
        let _ = fs2::FileExt::unlock(&self._lock);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ownership_release_is_not_delayed_by_an_inherited_descriptor() {
        let dir = std::env::temp_dir().join(format!(
            "cie-inherited-{}-{}",
            std::process::id(),
            crate::model::now()
        ));
        let storage = Storage::open(&dir).unwrap();
        let inherited = storage._lock.try_clone().unwrap();
        drop(storage);
        let next = Storage::open(&dir).unwrap();
        assert!(Storage::open(&dir).is_err());
        drop(inherited);
        assert!(Storage::open(&dir).is_err());
        drop(next);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
