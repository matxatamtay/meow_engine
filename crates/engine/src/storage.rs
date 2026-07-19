//! Origin-partitioned Web Storage with bounded local persistence.
#![cfg_attr(all(feature = "js-v8", not(feature = "js-boa")), allow(dead_code))]

use std::{
    cell::RefCell,
    collections::{BTreeMap, HashMap},
    error::Error,
    fmt, fs,
    path::{Path, PathBuf},
    rc::Rc,
};

use meow_url_policy::Origin;
use serde::{Deserialize, Serialize};

pub const DEFAULT_STORAGE_QUOTA_BYTES: usize = 5 * 1024 * 1024;

#[derive(Clone, Debug)]
pub(crate) struct StorageBindings {
    pub local: Option<Rc<RefCell<StorageArea>>>,
    pub session: Option<Rc<RefCell<StorageArea>>>,
}

#[derive(Debug)]
pub struct StorageManager {
    profile_dir: Option<PathBuf>,
    quota_bytes: usize,
    local: HashMap<Origin, Rc<RefCell<StorageArea>>>,
    session: HashMap<Origin, Rc<RefCell<StorageArea>>>,
}

impl StorageManager {
    #[must_use]
    pub fn ephemeral() -> Self {
        Self {
            profile_dir: None,
            quota_bytes: DEFAULT_STORAGE_QUOTA_BYTES,
            local: HashMap::new(),
            session: HashMap::new(),
        }
    }

    #[must_use]
    pub fn persistent(profile_dir: impl Into<PathBuf>) -> Self {
        let profile_dir = profile_dir.into();
        if let Err(error) = crate::prepare_profile(&profile_dir) {
            tracing::warn!(%error, profile = %profile_dir.display(), "profile preparation failed");
        }
        Self {
            profile_dir: Some(profile_dir),
            ..Self::ephemeral()
        }
    }

    pub(crate) fn bindings_for(&mut self, origin: &Origin) -> StorageBindings {
        if matches!(origin, Origin::Opaque) {
            return StorageBindings {
                local: None,
                session: None,
            };
        }
        let local = self.local.entry(origin.clone()).or_insert_with(|| {
            let path = self
                .profile_dir
                .as_ref()
                .map(|root| root.join("local-storage").join(origin_file_name(origin)));
            Rc::new(RefCell::new(StorageArea::load(path, self.quota_bytes)))
        });
        let session = self
            .session
            .entry(origin.clone())
            .or_insert_with(|| Rc::new(RefCell::new(StorageArea::empty(None, self.quota_bytes))));
        StorageBindings {
            local: Some(Rc::clone(local)),
            session: Some(Rc::clone(session)),
        }
    }
}

impl Default for StorageManager {
    fn default() -> Self {
        Self::ephemeral()
    }
}

#[derive(Debug)]
pub(crate) struct StorageArea {
    entries: BTreeMap<String, String>,
    persistence_path: Option<PathBuf>,
    quota_bytes: usize,
}

#[derive(Debug, Serialize, Deserialize)]
struct PersistedStorage {
    entries: BTreeMap<String, String>,
}

impl StorageArea {
    fn empty(persistence_path: Option<PathBuf>, quota_bytes: usize) -> Self {
        Self {
            entries: BTreeMap::new(),
            persistence_path,
            quota_bytes,
        }
    }

    fn load(path: Option<PathBuf>, quota_bytes: usize) -> Self {
        let Some(path) = path else {
            return Self::empty(None, quota_bytes);
        };
        let entries = fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<PersistedStorage>(&bytes).ok())
            .map(|storage| storage.entries)
            .unwrap_or_default();
        Self {
            entries,
            persistence_path: Some(path),
            quota_bytes,
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn key(&self, index: usize) -> Option<String> {
        self.entries.keys().nth(index).cloned()
    }

    pub fn get(&self, key: &str) -> Option<String> {
        self.entries.get(key).cloned()
    }

    pub fn set(&mut self, key: String, value: String) -> Result<(), StorageError> {
        let previous = self.entries.get(&key).cloned();
        self.entries.insert(key.clone(), value);
        if self.byte_len() > self.quota_bytes {
            if let Some(previous) = previous {
                self.entries.insert(key, previous);
            } else {
                self.entries.remove(&key);
            }
            return Err(StorageError::QuotaExceeded {
                limit: self.quota_bytes,
            });
        }
        self.persist()
    }

    pub fn remove(&mut self, key: &str) -> Result<(), StorageError> {
        if self.entries.remove(key).is_some() {
            self.persist()?;
        }
        Ok(())
    }

    pub fn clear(&mut self) -> Result<(), StorageError> {
        if !self.entries.is_empty() {
            self.entries.clear();
            self.persist()?;
        }
        Ok(())
    }

    fn byte_len(&self) -> usize {
        self.entries
            .iter()
            .map(|(key, value)| key.len().saturating_add(value.len()))
            .sum()
    }

    fn persist(&self) -> Result<(), StorageError> {
        let Some(path) = &self.persistence_path else {
            return Ok(());
        };
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(StorageError::Io)?;
        }
        let bytes = serde_json::to_vec(&PersistedStorage {
            entries: self.entries.clone(),
        })
        .map_err(StorageError::Serialize)?;
        let temporary = temporary_path(path);
        fs::write(&temporary, bytes).map_err(StorageError::Io)?;
        fs::rename(&temporary, path).map_err(StorageError::Io)
    }
}

#[derive(Debug)]
pub enum StorageError {
    QuotaExceeded { limit: usize },
    Io(std::io::Error),
    Serialize(serde_json::Error),
}

impl fmt::Display for StorageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::QuotaExceeded { limit } => {
                write!(formatter, "storage quota exceeded ({limit} bytes)")
            }
            Self::Io(error) => write!(formatter, "storage persistence failed: {error}"),
            Self::Serialize(error) => write!(formatter, "storage serialization failed: {error}"),
        }
    }
}

impl Error for StorageError {}

fn origin_file_name(origin: &Origin) -> String {
    let serialized = origin.to_string();
    let mut output = String::with_capacity(serialized.len() * 2 + 5);
    for byte in serialized.bytes() {
        use fmt::Write as _;
        write!(output, "{byte:02x}").expect("writing to String cannot fail");
    }
    output.push_str(".json");
    output
}

fn temporary_path(path: &Path) -> PathBuf {
    let mut name = path
        .file_name()
        .map_or_else(|| "storage".into(), |name| name.to_os_string());
    name.push(".tmp");
    path.with_file_name(name)
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    #[test]
    fn quota_failure_restores_the_previous_value() {
        let mut area = StorageArea::empty(None, 12);
        area.set("cat".to_owned(), "meow".to_owned()).unwrap();
        let error = area
            .set("cat".to_owned(), "this value is too large".to_owned())
            .unwrap_err();
        assert!(matches!(error, StorageError::QuotaExceeded { limit: 12 }));
        assert_eq!(area.get("cat").as_deref(), Some("meow"));
        assert!(
            area.set("another".to_owned(), "overflow".to_owned())
                .is_err()
        );
        assert_eq!(area.get("another"), None);
    }

    #[test]
    fn local_storage_persists_and_session_storage_does_not() {
        let root = std::env::temp_dir().join(format!(
            "meow-storage-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let origin = meow_url_policy::BrowserUrl::parse("https://example.test/")
            .unwrap()
            .origin();
        {
            let mut manager = StorageManager::persistent(&root);
            let bindings = manager.bindings_for(&origin);
            bindings
                .local
                .unwrap()
                .borrow_mut()
                .set("cat".to_owned(), "meow".to_owned())
                .unwrap();
            bindings
                .session
                .unwrap()
                .borrow_mut()
                .set("tab".to_owned(), "one".to_owned())
                .unwrap();
        }
        let mut manager = StorageManager::persistent(&root);
        let bindings = manager.bindings_for(&origin);
        assert_eq!(
            bindings.local.unwrap().borrow().get("cat").as_deref(),
            Some("meow")
        );
        assert_eq!(bindings.session.unwrap().borrow().get("tab"), None);
        let _ = fs::remove_dir_all(root);
    }
}
