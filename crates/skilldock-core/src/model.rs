use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Policy {
    #[ts(type = "'off' | 'notify' | 'auto'")]
    pub mode: String,
    pub interval_hours: u32,
}
impl Default for Policy {
    fn default() -> Self {
        Self {
            mode: "off".into(),
            interval_hours: 24,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Skill {
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
pub struct Settings {
    #[serde(default = "default_sites")]
    pub catalog_sites: Vec<String>,
    pub close_to_tray: bool,
    pub theme: String,
    pub update_mode: String,
    pub update_endpoint: String,
    pub update_public_key: String,
}
pub fn default_sites() -> Vec<String> {
    vec!["clawhub".into(), "skillhub".into(), "skills.sh".into()]
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            catalog_sites: default_sites(),
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
pub struct Snapshot {
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
            schema_version: 1,
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
pub struct PlanItem {
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
