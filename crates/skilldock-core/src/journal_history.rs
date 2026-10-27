use super::*;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
    time::SystemTime,
};

// Read-only projections: recovery and cleanup execution still read full Journals.
#[derive(Deserialize)]
pub(crate) struct JournalHistory {
    pub id: String,
    pub status: String,
    pub error: String,
    pub changes: Vec<Change>,
    pub before: HistorySnapshot,
    pub after: HistorySnapshot,
    #[serde(default)]
    pub object_cleanup: Option<HistoryCleanup>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct HistorySnapshot {
    pub skills: Vec<HistorySkill>,
    pub bindings: Vec<HistoryBinding>,
    pub presets: Vec<HistoryPreset>,
    targets: Vec<HistoryPath>,
    sources: Vec<HistorySource>,
    settings: HistorySettings,
    #[serde(default)]
    unmanaged_target_paths: Vec<String>,
    pub tasks: Vec<HistoryTask>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct HistorySkill {
    pub id: String,
    pub name: String,
    pub bundle_digest: String,
    pub external_path: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct HistoryBinding {
    pub path: String,
    pub digest: String,
    external_path: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct HistoryPreset {
    #[serde(default)]
    pub locks: BTreeMap<String, HistorySkill>,
}

#[derive(Deserialize)]
struct HistoryPath {
    path: String,
}

#[derive(Deserialize)]
struct HistorySource {
    kind: String,
    path: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct HistorySettings {
    #[serde(default = "default_history_profiles")]
    agent_profiles: Vec<HistoryProfile>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct HistoryProfile {
    user_paths: Vec<String>,
}

fn default_history_profiles() -> Vec<HistoryProfile> {
    model::default_agent_profiles()
        .into_iter()
        .map(|p| HistoryProfile {
            user_paths: p.user_paths,
        })
        .collect()
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct HistoryTask {
    pub id: String,
    pub created_at: String,
}

#[derive(Deserialize)]
pub(crate) struct HistoryCleanup {
    pub items: Vec<HistoryCleanupItem>,
}

#[derive(Deserialize)]
pub(crate) struct HistoryCleanupItem {
    pub choice: object_cleanup::CleanupChoice,
    pub status: String,
    pub reason: String,
}

impl HistorySnapshot {
    pub fn scan_roots(&self, changes: &[Change]) -> BTreeSet<String> {
        self.targets
            .iter()
            .map(|t| t.path.clone())
            .chain(
                self.settings
                    .agent_profiles
                    .iter()
                    .flat_map(|p| p.user_paths.clone()),
            )
            .chain(
                self.sources
                    .iter()
                    .filter(|s| {
                        matches!(
                            s.kind.as_str(),
                            "local" | "local_reference" | "local_managed"
                        )
                    })
                    .map(|s| s.path.clone()),
            )
            .chain(
                self.skills
                    .iter()
                    .chain(self.presets.iter().flat_map(|p| p.locks.values()))
                    .filter_map(|s| s.external_path.clone()),
            )
            .chain(self.bindings.iter().filter_map(|b| b.external_path.clone()))
            .chain(self.unmanaged_target_paths.iter().cloned())
            .chain(
                changes
                    .iter()
                    .filter_map(|c| c.path.parent())
                    .map(|p| p.display().to_string()),
            )
            .collect()
    }
}

#[derive(PartialEq, Eq)]
struct Stamp {
    length: u64,
    modified: SystemTime,
    created: Option<SystemTime>,
    #[cfg(unix)]
    identity: (u64, u64, i64, i64),
}

impl Stamp {
    fn read(path: &Path) -> Result<Self> {
        let meta = fs::metadata(path)?;
        Ok(Self {
            length: meta.len(),
            modified: meta.modified()?,
            created: meta.created().ok(),
            #[cfg(unix)]
            identity: {
                use std::os::unix::fs::MetadataExt;
                (meta.dev(), meta.ino(), meta.ctime(), meta.ctime_nsec())
            },
        })
    }
}

#[derive(Default)]
pub(crate) struct JournalCache {
    entries: BTreeMap<PathBuf, (Stamp, Arc<JournalHistory>)>,
}

impl Engine {
    pub(crate) fn all_journals(&self, root: &Path) -> Result<Vec<Arc<JournalHistory>>> {
        let paths: Vec<_> = fs::read_dir(root.join("transactions"))?
            .map(|entry| entry.map(|e| e.path()))
            .collect::<std::io::Result<Vec<_>>>()?
            .into_iter()
            .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("json"))
            .collect();
        let mut cache = self
            .journal_cache
            .lock()
            .map_err(|_| error::Error::Message("事务索引不可用，请重新启动应用".into()))?;
        cache.entries.retain(|path, _| paths.contains(path));
        let mut result = Vec::with_capacity(paths.len());
        for path in paths {
            let stamp = Stamp::read(&path)?;
            if let Some((known, journal)) = cache.entries.get(&path) {
                if *known == stamp {
                    result.push(journal.clone());
                    continue;
                }
            }
            let mut journal: JournalHistory = serde_json::from_slice(&fs::read(&path)?)?;
            if Stamp::read(&path)? != stamp {
                return fail("事务记录在读取期间变化，请重试");
            }
            // Only the task's original timestamp is needed by cleanup reports.
            journal.before.tasks.clear();
            journal.after.tasks.retain(|task| task.id == journal.id);
            let journal = Arc::new(journal);
            cache.entries.insert(path, (stamp, journal.clone()));
            result.push(journal);
        }
        Ok(result)
    }
}
