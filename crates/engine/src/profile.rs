//! Versioned browser profile manifest, migration, and corrupt-state recovery.

use std::{
    error::Error,
    fmt, fs, io,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};

pub const PROFILE_SCHEMA_VERSION: u32 = 2;
const PROFILE_MANIFEST: &str = "profile.json";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileManifest {
    pub schema_version: u32,
    pub engine_version: String,
    pub created_millis: u64,
    pub last_migrated_millis: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileMigrationReport {
    pub profile_dir: PathBuf,
    pub previous_version: Option<u32>,
    pub current_version: u32,
    pub created: bool,
    pub migrated: bool,
    pub recovered_corrupt_manifest: Option<PathBuf>,
    pub actions: Vec<String>,
}

pub fn prepare_profile(
    profile_dir: impl AsRef<Path>,
) -> Result<ProfileMigrationReport, ProfileError> {
    let profile_dir = profile_dir.as_ref();
    fs::create_dir_all(profile_dir)?;
    fs::create_dir_all(profile_dir.join("recovery"))?;
    fs::create_dir_all(profile_dir.join("crashes"))?;
    fs::create_dir_all(profile_dir.join("diagnostics"))?;
    let path = profile_dir.join(PROFILE_MANIFEST);
    let now = unix_millis();
    let mut report = ProfileMigrationReport {
        profile_dir: profile_dir.to_path_buf(),
        current_version: PROFILE_SCHEMA_VERSION,
        ..ProfileMigrationReport::default()
    };

    let mut manifest = if path.exists() {
        match fs::read(&path).map_err(ProfileError::Io).and_then(|bytes| {
            serde_json::from_slice::<ProfileManifest>(&bytes).map_err(ProfileError::Json)
        }) {
            Ok(manifest) => {
                report.previous_version = Some(manifest.schema_version);
                manifest
            }
            Err(_) => {
                let recovery = profile_dir
                    .join("recovery")
                    .join(format!("profile-corrupt-{}-{now}.json", std::process::id()));
                fs::rename(&path, &recovery)?;
                report.recovered_corrupt_manifest = Some(recovery.clone());
                report.actions.push(format!(
                    "moved corrupt profile manifest to {}",
                    recovery.display()
                ));
                report.created = true;
                ProfileManifest::new(now)
            }
        }
    } else {
        report.created = true;
        if profile_dir.join("local-storage").exists() {
            report.previous_version = Some(1);
            report.migrated = true;
            report
                .actions
                .push("adopted legacy local-storage profile as schema 1".to_owned());
        }
        ProfileManifest::new(now)
    };

    if manifest.schema_version > PROFILE_SCHEMA_VERSION {
        return Err(ProfileError::FutureVersion {
            found: manifest.schema_version,
            supported: PROFILE_SCHEMA_VERSION,
        });
    }
    if manifest.schema_version < PROFILE_SCHEMA_VERSION {
        report.previous_version = Some(manifest.schema_version);
        report.migrated = true;
        report.actions.push(format!(
            "migrated profile schema {} to {}",
            manifest.schema_version, PROFILE_SCHEMA_VERSION
        ));
        manifest.schema_version = PROFILE_SCHEMA_VERSION;
        manifest.last_migrated_millis = now;
    }
    manifest.engine_version = env!("CARGO_PKG_VERSION").to_owned();
    write_atomic_json(&path, &manifest)?;
    Ok(report)
}

impl ProfileManifest {
    fn new(now: u64) -> Self {
        Self {
            schema_version: PROFILE_SCHEMA_VERSION,
            engine_version: env!("CARGO_PKG_VERSION").to_owned(),
            created_millis: now,
            last_migrated_millis: now,
        }
    }
}

fn write_atomic_json(path: &Path, value: &impl Serialize) -> Result<(), ProfileError> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    let temporary = path.with_extension("json.tmp");
    fs::write(&temporary, bytes)?;
    fs::rename(temporary, path)?;
    Ok(())
}

fn unix_millis() -> u64 {
    u64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis(),
    )
    .unwrap_or(u64::MAX)
}

#[derive(Debug)]
pub enum ProfileError {
    Io(io::Error),
    Json(serde_json::Error),
    FutureVersion { found: u32, supported: u32 },
}

impl fmt::Display for ProfileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "profile I/O failed: {error}"),
            Self::Json(error) => write!(formatter, "profile manifest is invalid: {error}"),
            Self::FutureVersion { found, supported } => write!(
                formatter,
                "profile schema {found} is newer than supported schema {supported}"
            ),
        }
    }
}

impl Error for ProfileError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Json(error) => Some(error),
            Self::FutureVersion { .. } => None,
        }
    }
}

impl From<io::Error> for ProfileError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<serde_json::Error> for ProfileError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "meow-profile-{name}-{}-{}",
            std::process::id(),
            unix_millis()
        ))
    }

    #[test]
    fn creates_and_migrates_legacy_profile() {
        let root = temp("legacy");
        fs::create_dir_all(root.join("local-storage")).unwrap();
        let report = prepare_profile(&root).unwrap();
        assert!(report.created);
        assert!(report.migrated);
        assert_eq!(report.previous_version, Some(1));
        let manifest: ProfileManifest =
            serde_json::from_slice(&fs::read(root.join(PROFILE_MANIFEST)).unwrap()).unwrap();
        assert_eq!(manifest.schema_version, PROFILE_SCHEMA_VERSION);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn corrupt_manifest_is_recovered() {
        let root = temp("corrupt");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join(PROFILE_MANIFEST), b"not json").unwrap();
        let report = prepare_profile(&root).unwrap();
        assert!(report.recovered_corrupt_manifest.unwrap().is_file());
        assert!(root.join(PROFILE_MANIFEST).is_file());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn future_profile_is_rejected() {
        let root = temp("future");
        fs::create_dir_all(&root).unwrap();
        let manifest = ProfileManifest {
            schema_version: PROFILE_SCHEMA_VERSION + 1,
            engine_version: "future".to_owned(),
            created_millis: 0,
            last_migrated_millis: 0,
        };
        write_atomic_json(&root.join(PROFILE_MANIFEST), &manifest).unwrap();
        assert!(matches!(
            prepare_profile(&root),
            Err(ProfileError::FutureVersion { .. })
        ));
        let _ = fs::remove_dir_all(root);
    }
}
