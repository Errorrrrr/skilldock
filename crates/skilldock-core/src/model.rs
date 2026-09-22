use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Policy {
    #[ts(type = "'off' | 'notify' | 'auto'")]
    pub mode: String,
    pub interval_hours: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub daily_time: Option<String>,
}
impl Default for Policy {
    fn default() -> Self {
        Self {
            mode: "off".into(),
            interval_hours: 24,
            daily_time: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Skill {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub external_path: Option<String>,
    pub id: String,
    pub name: String,
    pub description: String,
    pub source_id: String,
    pub bundle_digest: String,
    pub relative_path: String,
    pub version: String,
    pub installed_at: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Source {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub local_member_ids: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub updates_removed: Option<bool>,
    pub id: String,
    pub name: String,
    pub kind: String,
    pub path: String,
    pub url: String,
    #[serde(default)]
    pub scan_subdir: String,
    pub reference: String,
    pub version: String,
    pub policy: Policy,
    pub last_checked: String,
    pub next_check: String,
    pub status: String,
    pub error: String,
}
impl Source {
    pub fn supports_remote_updates(&self) -> bool {
        if self.updates_removed == Some(true) || self.status == "detached" {
            return false;
        }
        match self.kind.as_str() {
            "git" => {
                reqwest::Url::parse(&self.url).is_ok_and(|url| {
                    matches!(url.scheme(), "https" | "ssh") && url.host_str().is_some()
                }) || (self.url.contains('@')
                    && self.url.contains(':')
                    && !self.url.starts_with('/'))
            }
            "catalog" | "clawhub" => !self.url.is_empty() && !self.reference.is_empty(),
            _ => false,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Target {
    pub id: String,
    pub name: String,
    pub tool: String,
    pub scope: String,
    pub path: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Binding {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub original_link: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub external_path: Option<String>,
    #[serde(default)]
    pub borrowed: bool,
    pub id: String,
    pub skill_id: String,
    pub target_id: String,
    pub path: String,
    pub version: String,
    pub digest: String,
    pub relative_path: String,
    pub claims: Vec<String>,
    pub follow: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Preset {
    pub id: String,
    pub name: String,
    pub description: String,
    pub skill_ids: Vec<String>,
    pub revision: u32,
    #[serde(default)]
    pub locks: std::collections::BTreeMap<String, Skill>,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub status: String,
    pub message: String,
    pub created_at: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AgentProfile {
    pub id: String,
    pub name: String,
    pub user_paths: Vec<String>,
    pub project_paths: Vec<String>,
}
pub fn default_agent_profiles() -> Vec<AgentProfile> {
    serde_json::from_str(include_str!("../../../shared/agent-profiles.json"))
        .expect("valid built-in agent profiles")
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct CatalogSite {
    pub name: String,
    pub url: String,
}
impl CatalogSite {
    pub fn legacy(value: &str) -> Self {
        let value = value.trim().trim_end_matches('/');
        let (name, url) = match value.to_ascii_lowercase().as_str() {
            "clawhub" | "clawhub.ai" | "https://clawhub.ai" => ("ClawHub", "https://clawhub.ai"),
            "skillhub" | "skillhub.cn" | "https://skillhub.cn" | "https://api.skillhub.cn" => {
                ("SkillHub", "https://skillhub.cn")
            }
            "skills.sh" | "https://skills.sh" | "https://www.skills.sh" => {
                ("Skills.sh", "https://skills.sh")
            }
            _ => (value, value),
        };
        Self {
            name: name.into(),
            url: url.into(),
        }
    }
}
impl<'de> Deserialize<'de> for CatalogSite {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // Existing state files and older CLI clients used string arrays.
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum StoredSite {
            Named { name: String, url: String },
            Legacy(String),
        }
        Ok(match StoredSite::deserialize(deserializer)? {
            StoredSite::Named { name, url } => Self { name, url },
            StoredSite::Legacy(value) => Self::legacy(&value),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct NetworkProxy {
    pub mode: String,
    pub url: String,
}
impl Default for NetworkProxy {
    fn default() -> Self {
        Self {
            mode: "system".into(),
            url: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    #[serde(default)]
    pub network_proxy: NetworkProxy,
    #[serde(default = "default_backup_retention")]
    pub backup_retention: u32,
    #[serde(default = "default_agent_profiles")]
    pub agent_profiles: Vec<AgentProfile>,
    #[serde(default = "default_sites")]
    pub catalog_sites: Vec<CatalogSite>,
    pub close_to_tray: bool,
    pub theme: String,
    pub update_mode: String,
    pub update_endpoint: String,
    pub update_public_key: String,
}
pub fn default_backup_retention() -> u32 {
    3
}
pub fn default_sites() -> Vec<CatalogSite> {
    ["clawhub", "skillhub", "skills.sh"]
        .into_iter()
        .map(CatalogSite::legacy)
        .collect()
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            network_proxy: NetworkProxy::default(),
            backup_retention: default_backup_retention(),
            catalog_sites: default_sites(),
            agent_profiles: default_agent_profiles(),
            close_to_tray: true,
            theme: "light".into(),
            update_mode: "off".into(),
            update_endpoint: String::new(),
            update_public_key: String::new(),
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PackageScope {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub auto_add: Option<bool>,
    #[serde(default)]
    pub origin_path: String,
    pub source_id: String,
    pub prefix: String,
    pub excluded: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SkillPackage {
    #[serde(default)]
    pub remote: bool,
    #[serde(default)]
    pub missing_member_ids: Vec<String>,
    #[serde(default)]
    pub issues: Vec<String>,
    pub id: String,
    pub name: String,
    pub path: String,
    pub scopes: Vec<PackageScope>,
    pub member_ids: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PresetPackage {
    pub preset_id: String,
    pub package_id: String,
    pub auto_add: bool,
    pub excluded_ids: Vec<String>,
    pub selected_ids: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PresetApplication {
    pub preset_id: String,
    pub target_id: String,
    pub follow: bool,
    pub applied_revision: u32,
    pub error: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ExternalInstallation {
    pub skill_id: String,
    pub target_id: String,
    pub path: String,
    pub entity_path: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ContentBackup {
    #[serde(default)]
    pub added_skill_ids: Vec<String>,
    pub source_id: String,
    pub created_at: String,
    pub skills: Vec<Skill>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    /// 最近一次更新前的完整来源成员；只用于撤销更新，不提供版本分发。
    #[serde(default)]
    pub content_backups: Vec<ContentBackup>,
    /// 合并收录时保留来源追溯，不创建额外实体。
    #[serde(default)]
    pub skill_origins: std::collections::BTreeMap<String, Vec<String>>,
    /// 名称入口的所有权清单，禁止覆盖用户自行放入的文件。
    #[serde(default)]
    pub library_entries: std::collections::BTreeMap<String, String>,
    // Derived from the current filesystem; never trust a persisted observation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub skill_entity_paths: Option<std::collections::BTreeMap<String, String>>,
    #[serde(default)]
    pub unmanaged_target_paths: Vec<String>,
    #[serde(default)]
    pub packages: Vec<SkillPackage>,
    #[serde(default)]
    pub preset_packages: Vec<PresetPackage>,
    #[serde(default)]
    pub preset_applications: Vec<PresetApplication>,
    #[serde(default)]
    pub external_installations: Vec<ExternalInstallation>,
    pub initialized: bool,
    pub storage_root: String,
    pub revision: u32,
    pub skills: Vec<Skill>,
    pub sources: Vec<Source>,
    pub targets: Vec<Target>,
    pub bindings: Vec<Binding>,
    pub presets: Vec<Preset>,
    pub tasks: Vec<Task>,
    pub settings: Settings,
    #[serde(default = "schema_version")]
    pub schema_version: u32,
}
pub fn schema_version() -> u32 {
    1
}
impl Snapshot {
    pub fn empty(root: String) -> Self {
        Self {
            skill_entity_paths: None,
            content_backups: Vec::new(),
            skill_origins: Default::default(),
            library_entries: Default::default(),
            unmanaged_target_paths: Vec::new(),
            packages: vec![],
            preset_packages: vec![],
            preset_applications: vec![],
            external_installations: vec![],
            initialized: false,
            storage_root: root,
            revision: 0,
            skills: vec![],
            sources: vec![],
            targets: vec![],
            bindings: vec![],
            presets: vec![],
            tasks: vec![],
            settings: Settings::default(),
            schema_version: 2,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ScanItem {
    pub path: String,
    pub name: String,
    pub description: String,
    pub status: String,
    pub error: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ScanResult {
    pub root: String,
    pub items: Vec<ScanItem>,
    pub warnings: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CatalogItem {
    pub slug: String,
    pub name: String,
    pub description: String,
    pub version: String,
    pub site: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct CatalogResult {
    pub items: Vec<CatalogItem>,
    pub errors: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PlanReplacement {
    pub next_entity_path: String,
    pub next_version: String,
    pub binding_id: String,
    pub skill_id: String,
    pub entity_path: String,
    pub version: String,
    pub claims: Vec<String>,
    pub blocking_claims: Vec<String>,
    pub content_equal: Option<bool>,
    pub restores_original: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PlanItem {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub replacement: Option<PlanReplacement>,
    pub skill_id: String,
    pub target_id: String,
    pub path: String,
    pub action: String,
    pub error: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct DistributionPlan {
    pub items: Vec<PlanItem>,
    pub revision: u32,
}
