pub mod error;
pub mod files;
pub mod model;
pub mod network;
mod operations;
mod portable;

use error::{Result, fail};
use files::Change;
use model::*;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone)]
pub struct Engine {
    pub config_dir: PathBuf,
}
#[derive(Serialize, Deserialize)]
struct Config {
    storage_root: String,
}
#[derive(Serialize, Deserialize)]
struct Journal {
    id: String,
    status: String,
    changes: Vec<Change>,
    before: Snapshot,
    after: Snapshot,
    error: String,
}
pub fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}
pub fn id() -> String {
    uuid::Uuid::new_v4().to_string()
}
pub(crate) fn text<'a>(v: &'a Value, key: &str) -> Result<&'a str> {
    v.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| error::Error::Message(format!("缺少参数：{key}")))
}
pub(crate) fn strings(v: &Value, key: &str) -> Result<Vec<String>> {
    serde_json::from_value(v.get(key).cloned().unwrap_or(json!([]))).map_err(Into::into)
}
pub(crate) fn optional<'a>(v: &'a Value, key: &str, default: &'a str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap_or(default)
}
pub(crate) fn flag(v: &Value, key: &str) -> bool {
    v.get(key).and_then(Value::as_bool).unwrap_or(false)
}
pub(crate) fn skill_path(root: &Path, skill: &Skill) -> PathBuf {
    root.join("objects")
        .join(&skill.bundle_digest)
        .join("tree")
        .join(&skill.relative_path)
}
pub(crate) fn binding_path(root: &Path, b: &Binding) -> PathBuf {
    root.join("objects")
        .join(&b.digest)
        .join("tree")
        .join(&b.relative_path)
}

impl Engine {
    pub fn update_guard(&self) -> Result<fs::File> {
        self.lock()
    }
    pub fn new(config_dir: Option<PathBuf>) -> Result<Self> {
        Ok(Self {
            config_dir: config_dir.unwrap_or(files::default_config()?),
        })
    }
    fn lock(&self) -> Result<fs::File> {
        fs::create_dir_all(&self.config_dir)?;
        let file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(self.config_dir.join("control.lock"))?;
        fs4::FileExt::try_lock(&file).map_err(|_| {
            error::Error::Message("另一个 SkillDock 操作正在写入，请稍后重试".into())
        })?;
        Ok(file)
    }
    fn root(&self) -> Result<Option<PathBuf>> {
        let path = self.config_dir.join("config.json");
        if !path.exists() {
            return Ok(None);
        }
        let config: Config = serde_json::from_slice(&fs::read(path)?)?;
        let root = files::absolute(&config.storage_root)?;
        if !root.join("state.json").is_file() {
            return fail("已配置的统一目录不可用，请重新连接或恢复；未创建第二份库");
        }
        Ok(Some(root))
    }
    pub fn snapshot(&self) -> Result<Snapshot> {
        let Some(root) = self.root()? else {
            return Ok(Snapshot::empty(
                files::default_root()?.display().to_string(),
            ));
        };
        let mut state: Snapshot = serde_json::from_slice(&fs::read(root.join("state.json"))?)?;
        if state.schema_version != 1 {
            return fail("数据版本不兼容，请升级 SkillDock");
        }
        if state.storage_root != root.display().to_string() {
            return fail("数据根指针与库记录不一致，请恢复迁移");
        }
        if let Ok(entries) = fs::read_dir(root.join("transactions")) {
            for entry in entries.flatten() {
                if entry.path().extension().and_then(|s| s.to_str()) != Some("json") {
                    continue;
                }
                let journal: Journal = serde_json::from_slice(&fs::read(entry.path())?)?;
                if matches!(journal.status.as_str(), "running" | "needsRecovery")
                    && !state
                        .tasks
                        .iter()
                        .any(|t| t.id == journal.id && t.status == "success")
                {
                    state.tasks.retain(|t| t.id != journal.id);
                    state.tasks.push(Task {
                        id: journal.id,
                        kind: "recovery".into(),
                        title: "有未完成的文件事务".into(),
                        status: "needsRecovery".into(),
                        message: journal.error,
                        created_at: now(),
                    });
                }
            }
        }
        self.decorate_migration(&mut state)?;
        Ok(state)
    }
    pub fn configure(&self, path: &str) -> Result<Snapshot> {
        let _guard = self.lock()?;
        if self.root()?.is_some() {
            return fail("已经设置统一目录，请通过迁移功能更改");
        }
        let root = files::absolute(path)?;
        Self::check_root(&root)?;
        if root.exists() && fs::read_dir(&root)?.next().is_some() {
            return fail("新统一目录必须为空，避免接管未知内容");
        }
        fs::create_dir_all(&root)?;
        let root = fs::canonicalize(root)?;
        Self::check_root(&root)?;
        for sub in ["objects", "transactions", "backups", "cache", "trash"] {
            fs::create_dir_all(root.join(sub))?;
        }
        let mut state = Snapshot::empty(root.display().to_string());
        state.initialized = true;
        files::atomic_json(&root.join("state.json"), &state)?;
        files::atomic_json(
            &self.config_dir.join("config.json"),
            &Config {
                storage_root: state.storage_root.clone(),
            },
        )?;
        Ok(state)
    }
    fn check_root(root: &Path) -> Result<()> {
        if root == files::home()? || root.parent().is_none() || files::protected(root) {
            return fail("请选择专用的 SkillDock 数据目录");
        }
        for t in Self::discover()? {
            let p = PathBuf::from(t.path);
            if root.starts_with(&p) || p.starts_with(root) {
                return fail("统一目录不能位于工具扫描根内或包含工具扫描根");
            }
        }
        Ok(())
    }
    pub fn discover() -> Result<Vec<Target>> {
        let h = files::home()?;
        Ok([
            ("通用 Agent Skills", "agents", ".agents/skills"),
            ("Codex", "codex", ".codex/skills"),
            ("Claude Code", "claude", ".claude/skills"),
            ("Cursor", "cursor", ".cursor/skills"),
            ("OpenCode", "opencode", ".config/opencode/skills"),
            ("Gemini CLI", "gemini", ".gemini/skills"),
            ("OpenClaw", "openclaw", ".openclaw/skills"),
        ]
        .into_iter()
        .map(|(name, tool, path)| Target {
            id: format!("discovered-{path}"),
            name: name.into(),
            tool: tool.into(),
            scope: "user".into(),
            path: h.join(path).display().to_string(),
        })
        .filter(|t| Path::new(&t.path).is_dir())
        .collect())
    }
    pub(crate) fn transact(
        &self,
        kind: &str,
        title: &str,
        mutate: impl FnOnce(&mut Snapshot, &Path, &mut Vec<Change>) -> Result<()>,
    ) -> Result<Snapshot> {
        let _guard = self.lock()?;
        let root = self
            .root()?
            .ok_or_else(|| error::Error::Message("请先设置统一目录".into()))?;
        let library_lock = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(root.join("manager.lock"))?;
        fs4::FileExt::try_lock(&library_lock)
            .map_err(|_| error::Error::Message("中央库正在使用".into()))?;
        let before = self.snapshot()?;
        if before.tasks.iter().any(|t| t.status == "needsRecovery") {
            return fail("请先恢复未完成的文件事务");
        }
        let mut after = before.clone();
        let mut changes = vec![];
        mutate(&mut after, &root, &mut changes)?;
        let task_id = id();
        after.revision = before
            .revision
            .checked_add(1)
            .ok_or_else(|| error::Error::Message("版本计数已满".into()))?;
        after.tasks.push(Task {
            id: task_id.clone(),
            kind: kind.into(),
            title: title.into(),
            status: if kind == "update_error" {
                "failed"
            } else {
                "success"
            }
            .into(),
            message: if kind == "update_error" {
                "来源检查失败，原安装已保留；请查看来源详情".into()
            } else {
                format!("完成 {} 项文件变更", changes.len())
            },
            created_at: now(),
        });
        let journal_path = root.join("transactions").join(format!("{task_id}.json"));
        let mut journal = Journal {
            id: task_id.clone(),
            status: "running".into(),
            changes,
            before: before.clone(),
            after: after.clone(),
            error: String::new(),
        };
        files::atomic_json(&journal_path, &journal)?;
        let result = (|| -> Result<()> {
            for change in &journal.changes {
                files::apply(change)?;
            }
            files::atomic_json(&root.join("state.json"), &after)
        })();
        if let Err(error) = result {
            let mut failures = vec![];
            for change in journal.changes.iter().rev() {
                if let Err(e) = files::undo(change) {
                    failures.push(e.to_string());
                }
            }
            journal.status = if failures.is_empty() {
                "rolledBack"
            } else {
                "needsRecovery"
            }
            .into();
            journal.error = format!("{error} {}", failures.join("；"));
            let mut failed = before;
            failed.revision = after.revision;
            failed.tasks.push(Task {
                id: task_id,
                kind: kind.into(),
                title: title.into(),
                status: if failures.is_empty() {
                    "failed"
                } else {
                    "needsRecovery"
                }
                .into(),
                message: journal.error.clone(),
                created_at: now(),
            });
            files::atomic_json(&root.join("state.json"), &failed)?;
            files::atomic_json(&journal_path, &journal)?;
            return fail(journal.error);
        }
        journal.status = "committed".into();
        files::atomic_json(&journal_path, &journal)?;
        Ok(after)
    }
    pub fn recover(&self, task_id: &str) -> Result<Snapshot> {
        if let Some(state) = self.recover_migration(task_id)? {
            return Ok(state);
        }
        let _guard = self.lock()?;
        let root = self
            .root()?
            .ok_or_else(|| error::Error::Message("尚未初始化".into()))?;
        let task_id = uuid::Uuid::parse_str(task_id)
            .map_err(|_| error::Error::Message("无效任务 ID".into()))?;
        let path = root.join("transactions").join(format!("{task_id}.json"));
        let mut journal: Journal = serde_json::from_slice(&fs::read(&path)?)?;
        if !matches!(journal.status.as_str(), "running" | "needsRecovery") {
            return fail("该任务不需要恢复");
        }
        let mut state: Snapshot = serde_json::from_slice(&fs::read(root.join("state.json"))?)?;
        if state
            .tasks
            .iter()
            .any(|t| t.id == journal.id && t.status == "success")
        {
            journal.status = "committed".into();
            files::atomic_json(&path, &journal)?;
            return Ok(state);
        }
        for change in journal.changes.iter().rev() {
            files::undo(change)?;
        }
        journal.status = "rolledBack".into();
        files::atomic_json(&path, &journal)?;
        state.tasks.retain(|t| t.id != journal.id);
        state.tasks.push(Task {
            id: id(),
            kind: "recovery".into(),
            title: "已恢复文件事务".into(),
            status: "success".into(),
            message: journal.id,
            created_at: now(),
        });
        state.revision += 1;
        files::atomic_json(&root.join("state.json"), &state)?;
        Ok(state)
    }
    pub async fn execute(&self, request: Value) -> Result<Value> {
        let action = text(&request, "action")?.to_string();
        match action.as_str() {
            "snapshot" => Ok(serde_json::to_value(self.snapshot()?)?),
            "configure" => Ok(serde_json::to_value(
                self.configure(text(&request, "path")?)?,
            )?),
            "scan" => Ok(serde_json::to_value(files::scan(&files::absolute(
                text(&request, "path")?,
            )?)?)?),
            "discover" => Ok(serde_json::to_value(Self::discover()?)?),
            "skill_history" => {
                let state = self.snapshot()?;
                let sid = text(&request, "skillId")?;
                let current = state
                    .skills
                    .iter()
                    .find(|s| s.id == sid)
                    .cloned()
                    .ok_or_else(|| error::Error::Message("Skill 不存在".into()))?;
                let mut versions = vec![current];
                for entry in fs::read_dir(Path::new(&state.storage_root).join("transactions"))? {
                    let path = entry?.path();
                    if path.extension().and_then(|s| s.to_str()) != Some("json") {
                        continue;
                    }
                    let journal: Journal = serde_json::from_slice(&fs::read(path)?)?;
                    for skill in journal
                        .before
                        .skills
                        .into_iter()
                        .chain(journal.after.skills)
                    {
                        if skill.id == sid
                            && !versions
                                .iter()
                                .any(|v| v.bundle_digest == skill.bundle_digest)
                            && skill_path(Path::new(&state.storage_root), &skill)
                                .join("SKILL.md")
                                .is_file()
                        {
                            versions.push(skill);
                        }
                    }
                }
                Ok(serde_json::to_value(versions)?)
            }
            "read_skill" => {
                let state = self.snapshot()?;
                let skill = state
                    .skills
                    .iter()
                    .find(|s| s.id == optional(&request, "skillId", ""))
                    .ok_or_else(|| error::Error::Message("Skill 不存在".into()))?;
                Ok(json!(fs::read_to_string(
                    skill_path(Path::new(&state.storage_root), skill).join("SKILL.md")
                )?))
            }
            "search_catalog" => Ok(serde_json::to_value(
                network::search(text(&request, "query")?, &strings(&request, "sites")?).await?,
            )?),
            "import_git" | "install_catalog" => {
                let root = self
                    .root()?
                    .ok_or_else(|| error::Error::Message("请先设置统一目录".into()))?;
                let prepared = if action == "import_git" {
                    network::prepare_git(
                        &root.join("cache"),
                        text(&request, "url")?,
                        optional(&request, "reference", "HEAD"),
                        optional(&request, "subdir", ""),
                    )
                    .await?
                } else {
                    network::prepare_catalog(
                        &root.join("cache"),
                        text(&request, "slug")?,
                        optional(&request, "site", "clawhub"),
                    )
                    .await?
                };
                let mut info = prepared_info(&prepared);
                let mut selected = vec![];
                if action == "import_git" {
                    info.scan_subdir = optional(&request, "subdir", "").replace('\\', "/");
                }
                {
                    if !info.scan_subdir.is_empty() {
                        selected = files::scan(&prepared.path.join(&info.scan_subdir))?
                            .items
                            .into_iter()
                            .filter(|i| i.status == "ready")
                            .map(|i| i.path)
                            .collect();
                        if selected.is_empty() {
                            return fail("Git 子目录中未发现 Skill");
                        }
                    }
                }
                let state = self.import_folder(&prepared.path, selected, false, Some(info))?;
                Ok(serde_json::to_value(state)?)
            }
            "check_source" => Ok(serde_json::to_value(
                self.check_source(text(&request, "sourceId")?, flag(&request, "apply"))
                    .await?,
            )?),
            "recover" => Ok(serde_json::to_value(
                self.recover(text(&request, "taskId")?)?,
            )?),
            "migrate_storage" => Ok(serde_json::to_value(
                self.migrate_storage(text(&request, "path")?)?,
            )?),
            "export_preset" => {
                self.export_preset(text(&request, "presetId")?, text(&request, "path")?)
            }
            "import_preset" => Ok(serde_json::to_value(
                self.import_preset(text(&request, "path")?)?,
            )?),
            _ => self.execute_local(&action, &request),
        }
    }
}
pub(crate) fn prepared_info(p: &network::PreparedSource) -> Source {
    Source {
        id: id(),
        name: p.name.clone(),
        kind: p.kind.clone(),
        path: p.path.display().to_string(),
        url: p.url.clone(),
        reference: p.reference.clone(),
        scan_subdir: p.scan_subdir.clone(),
        version: p.version.clone(),
        policy: Policy::default(),
        last_checked: String::new(),
        next_check: String::new(),
        status: "current".into(),
        error: String::new(),
    }
}
