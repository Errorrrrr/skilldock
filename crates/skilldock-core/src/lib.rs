mod backups;
pub mod error;
pub mod files;
mod git_packages;
mod local_presets;
mod local_sources;
pub mod model;
pub mod network;
mod object_cleanup;
mod operations;
mod packages;
mod portable;
mod schedule;
mod single_content;
mod source_binding;
mod system_proxy;

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
    #[serde(default)]
    pruning: Option<Vec<backups::BackupRemoval>>,
    #[serde(default)]
    object_cleanup: Option<object_cleanup::ObjectCleanup>,
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
    if let Some(path) = &skill.external_path {
        return PathBuf::from(path);
    }
    root.join("objects")
        .join(&skill.bundle_digest)
        .join("tree")
        .join(&skill.relative_path)
}
pub(crate) fn binding_path(root: &Path, b: &Binding) -> PathBuf {
    if let Some(path) = &b.external_path {
        return PathBuf::from(path);
    }
    root.join("objects")
        .join(&b.digest)
        .join("tree")
        .join(&b.relative_path)
}

pub(crate) fn binding_matches(root: &Path, binding: &Binding) -> bool {
    if binding.borrowed {
        Path::new(&binding.path)
            .ancestors()
            .any(|path| fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink()))
            && fs::canonicalize(&binding.path).ok().is_some_and(|path| {
                Some(path) == fs::canonicalize(binding_path(root, binding)).ok()
            })
    } else {
        fs::read_link(&binding.path).ok() == Some(binding_path(root, binding))
    }
}

impl Engine {
    #[cfg(test)]
    pub(crate) fn configure_legacy_fixture(&self, path: &str) -> Result<Snapshot> {
        let mut state = self.configure(path)?;
        let internal = PathBuf::from(&state.storage_root);
        let public = internal.parent().unwrap().to_path_buf();
        for entry in fs::read_dir(&internal)? {
            let entry = entry?;
            fs::rename(entry.path(), public.join(entry.file_name()))?;
        }
        fs::remove_dir(internal)?;
        state.schema_version = 2;
        state.storage_root = public.display().to_string();
        files::atomic_json(&public.join("state.json"), &state)?;
        files::atomic_json(
            &self.config_dir.join("config.json"),
            &Config {
                storage_root: state.storage_root.clone(),
            },
        )?;
        Ok(state)
    }
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
        // Older folder imports inferred a remote source from .git. Keep them local.
        let local_sources: std::collections::BTreeSet<_> = state
            .packages
            .iter()
            .filter(|p| !p.remote)
            .flat_map(|p| p.scopes.iter().map(|scope| scope.source_id.clone()))
            .collect();
        for source in &mut state.sources {
            if source.kind == "git" && local_sources.contains(&source.id) {
                source.kind = "local".into();
                source.url.clear();
                source.reference.clear();
            }
            if !source.supports_remote_updates() {
                source.policy.mode = "off".into();
                source.next_check.clear();
            }
        }
        Self::refresh_python_diagnostics(&mut state);
        Self::observe_installations(&mut state);
        if !matches!(state.schema_version, 1 | 2 | 3) {
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
        state
            .settings
            .agent_profiles
            .retain(|profile| !profile.id.eq_ignore_ascii_case("qclaw"));
        for target in &mut state.targets {
            if target.tool == "custom" && target.name.eq_ignore_ascii_case("skills") {
                let inferred = operations::inferred_target(
                    Path::new(&target.path),
                    &state.settings.agent_profiles,
                );
                target.name = inferred.name;
                target.tool = inferred.tool;
                target.scope = inferred.scope;
            }
        }
        self.decorate_object_cleanups(&root, &mut state)?;
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
        let root = root.join(".skilldock");
        fs::create_dir_all(&root)?;
        for sub in ["objects", "transactions", "backups", "cache", "trash"] {
            fs::create_dir_all(root.join(sub))?;
        }
        let mut state = Snapshot::empty(root.display().to_string());
        state.initialized = true;
        state.schema_version = 3;
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
        Self::discover_profiles(&default_agent_profiles())
    }
    fn discover_profiles(profiles: &[AgentProfile]) -> Result<Vec<Target>> {
        let mut targets: Vec<Target> = vec![];
        for profile in profiles {
            if profile.id.eq_ignore_ascii_case("qclaw") {
                continue;
            }
            for path in &profile.user_paths {
                let path = files::absolute(path)?;
                if !path.is_dir() {
                    continue;
                }
                targets.push(Target {
                    id: format!("discovered-{}-{}", profile.id, path.display()),
                    name: profile.name.clone(),
                    tool: profile.id.clone(),
                    scope: "user".into(),
                    path: path.display().to_string(),
                });
            }
        }
        Ok(targets)
    }
    pub fn discover_configured(&self) -> Result<Vec<Target>> {
        let snapshot = self.snapshot()?;
        let mut targets = Self::discover_profiles(&snapshot.settings.agent_profiles)?;
        for target in snapshot.targets {
            if target.tool.eq_ignore_ascii_case("qclaw") {
                continue;
            }
            if Path::new(&target.path).is_dir()
                && !targets.iter().any(|t| {
                    t.tool == target.tool && t.scope == target.scope && t.path == target.path
                })
            {
                targets.push(target);
            }
        }
        Ok(targets)
    }
    pub(crate) fn transact(
        &self,
        kind: &str,
        title: &str,
        mutate: impl FnOnce(&mut Snapshot, &Path, &mut Vec<Change>) -> Result<()>,
    ) -> Result<Snapshot> {
        self.transact_with_cleanup(kind, title, |state, root, changes, _| {
            mutate(state, root, changes)
        })
    }
    pub(crate) fn transact_with_cleanup(
        &self,
        kind: &str,
        title: &str,
        mutate: impl FnOnce(
            &mut Snapshot,
            &Path,
            &mut Vec<Change>,
            &mut Option<object_cleanup::ObjectCleanup>,
        ) -> Result<()>,
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
        after.schema_version = before.schema_version.max(2);
        let mut changes = vec![];
        let mut object_cleanup = None;
        mutate(&mut after, &root, &mut changes, &mut object_cleanup)?;
        single_content::finalize(&before, &mut after, &root, &mut changes, kind)?;
        single_content::plan_entries(&before, &mut after, &root, &mut changes)?;
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
            object_cleanup,
            pruning: None,
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
        if matches!(kind, "import" | "settings" | "backup_cleanup") {
            let cleanup = self.prune_backups(&root, &after);
            for task in after
                .tasks
                .iter_mut()
                .filter(|t| t.kind == "backup_cleanup")
            {
                if cleanup.is_ok() {
                    task.status = "success".into();
                    task.message = "旧备份清理完成".into();
                }
            }
            if let Err(error) = cleanup {
                after.tasks.retain(|t| t.kind != "backup_cleanup");
                after.tasks.push(Task {
                    id: id(),
                    kind: "backup_cleanup".into(),
                    title: "部分旧备份未清理".into(),
                    status: "failed".into(),
                    message: error.to_string(),
                    created_at: now(),
                });
            }
            files::atomic_json(&root.join("state.json"), &after)?;
        }
        let retained_digests = after
            .skills
            .iter()
            .chain(after.content_backups.iter().flat_map(|b| &b.skills))
            .map(|s| &s.bundle_digest)
            .collect::<std::collections::BTreeSet<_>>();
        let retired_content = before
            .skills
            .iter()
            .chain(before.content_backups.iter().flat_map(|b| &b.skills))
            .any(|s| !s.bundle_digest.is_empty() && !retained_digests.contains(&s.bundle_digest));
        if after.schema_version >= 3
            && journal.object_cleanup.is_none()
            && (retired_content
                || kind == "enable_single_content"
                || (kind == "add_target" && !journal.changes.is_empty()))
        {
            // Cleanup is forward-only and starts only after the new content is committed.
            let cleanup = (|| -> Result<()> {
                let (plan, scan) = self.history_cleanup_plan(&root, &after)?;
                let choices = plan
                    .items
                    .iter()
                    .filter(|item| item.status == "ready")
                    .map(|item| object_cleanup::CleanupChoice {
                        digest: item.digest.clone(),
                        fingerprint: item.fingerprint.clone(),
                    })
                    .collect::<Vec<_>>();
                if !choices.is_empty() {
                    journal.object_cleanup =
                        Some(self.prepare_object_cleanup(&root, &scan, &[], &plan, choices)?);
                    files::atomic_json(&journal_path, &journal)?;
                }
                Ok(())
            })();
            if let Err(error) = cleanup {
                after.tasks.push(Task {
                    id: id(),
                    kind: "object_cleanup_plan".into(),
                    title: "当前内容已保存，旧内容清理未完成".into(),
                    status: "failed".into(),
                    message: format!("{error}；可在设置中重新预览并清理旧内容"),
                    created_at: now(),
                });
                files::atomic_json(&root.join("state.json"), &after)?;
            }
        }
        if journal.object_cleanup.is_some() {
            // Forward-only cleanup: never enter the file-transaction undo branch.
            self.run_object_cleanup(&root, &after, &mut journal)?;
            return self.snapshot();
        }
        Self::observe_installations(&mut after);
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
        let library_lock = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(root.join("manager.lock"))?;
        fs4::FileExt::try_lock(&library_lock)
            .map_err(|_| error::Error::Message("中央库正在使用".into()))?;
        let mut state: Snapshot = serde_json::from_slice(&fs::read(root.join("state.json"))?)?;
        let stale_rollback = journal.status == "rolledBack"
            && state
                .tasks
                .iter()
                .any(|t| t.id == journal.id && t.status == "needsRecovery");
        if !matches!(journal.status.as_str(), "running" | "needsRecovery") && !stale_rollback {
            return fail("该任务不需要恢复");
        }
        if state
            .tasks
            .iter()
            .any(|t| t.id == journal.id && t.status == "success")
        {
            journal.status = "committed".into();
            files::atomic_json(&path, &journal)?;
            return Ok(state);
        }
        if !stale_rollback {
            for change in journal.changes.iter().rev() {
                files::undo(change)?;
            }
        }
        state
            .tasks
            .retain(|t| t.id != journal.id && !(t.kind == "recovery" && t.message == journal.id));
        state.tasks.push(Task {
            id: id(),
            kind: "recovery".into(),
            title: "已恢复文件事务".into(),
            status: "success".into(),
            message: journal.id.clone(),
            created_at: now(),
        });
        state.revision = state
            .revision
            .checked_add(1)
            .ok_or_else(|| error::Error::Message("版本计数已满".into()))?;
        // Save the recovered state first. If interrupted, the still-open journal
        // allows an idempotent retry instead of leaving an unrecoverable task.
        files::atomic_json(&root.join("state.json"), &state)?;
        journal.status = "rolledBack".into();
        files::atomic_json(&path, &journal)?;
        Ok(state)
    }
    pub async fn execute(&self, request: Value) -> Result<Value> {
        let proxy = self.snapshot()?.settings.network_proxy;
        network::with_proxy(proxy, self.execute_request(request)).await
    }
    async fn execute_request(&self, request: Value) -> Result<Value> {
        let action = text(&request, "action")?.to_string();
        match action.as_str() {
            "preview_single_content" => self.preview_single_content(),
            "arrange_library" => {
                let state = self.snapshot()?;
                if state.schema_version < 3 {
                    return fail("请先确认单一内容迁移");
                }
                if Path::new(&state.storage_root)
                    .file_name()
                    .is_some_and(|n| n == ".skilldock")
                {
                    return Ok(serde_json::to_value(state)?);
                }
                Ok(serde_json::to_value(
                    self.migrate_storage(&state.storage_root)?,
                )?)
            }
            "enable_single_content" => self.enable_single_content(&request),
            "undo_content_update" => self.undo_content_update(&request),
            "replace_current_content" => self.replace_current_content(&request),
            "preview_bind_source" => self.bind_source(&request, true).await,
            "bind_source" => self.bind_source(&request, false).await,
            "preview_git_package" => self.preview_git_package(&request).await,
            "import_git_package" => self.import_git_package(&request),
            "import_git" => {
                let preview = self.preview_git_package(&request).await?;
                let result = self.import_git_package(
                    &json!({"token":preview["token"],"revision":preview["revision"]}),
                )?;
                Ok(result["snapshot"].clone())
            }
            "import_package" => self.import_package(&request),
            "save_preset" if flag(&request, "syncApplied") => {
                self.execute_local("save_preset", &request)?;
                self.reconcile_packages()?;
                Ok(serde_json::to_value(self.snapshot()?)?)
            }
            "package_migration_preview" => self.package_migration_preview(&request),
            "retry_preset_sync" => {
                self.reconcile_packages()?;
                Ok(serde_json::to_value(self.snapshot()?)?)
            }
            "preview_local_source" => self.preview_preset_folder(&request),
            "save_local_source" => self.save_local_source(&request),
            "apply_local_source" | "revoke_local_source" | "remove_local_source" => {
                self.manage_local_source(&request)
            }
            "preview_preset_folder" => self.preview_preset_folder(&request),
            "import_preset_folder" => self.import_preset_folder(&request),
            "test_proxy" => {
                let proxy: NetworkProxy = serde_json::from_value(request["networkProxy"].clone())?;
                network::test_proxy(proxy).await
            }
            "snapshot" => Ok(serde_json::to_value(self.snapshot()?)?),
            "configure" => Ok(serde_json::to_value(
                self.configure(text(&request, "path")?)?,
            )?),
            "scan_many" => Ok(serde_json::to_value(files::scan_many(&strings(
                &request, "paths",
            )?)?)?),
            "scan" => Ok(serde_json::to_value(files::scan(&files::absolute(
                text(&request, "path")?,
            )?)?)?),
            "discover" => Ok(serde_json::to_value(self.discover_configured()?)?),
            "skill_history" => {
                let state = self.snapshot()?;
                let sid = text(&request, "skillId")?;
                let current = state
                    .skills
                    .iter()
                    .find(|s| s.id == sid)
                    .cloned()
                    .ok_or_else(|| error::Error::Message("Skill 不存在".into()))?;
                if state.schema_version >= 3 {
                    return fail("当前模式不提供历史版本分发，请使用撤销上次更新");
                }
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
            "install_catalog" => {
                let root = self
                    .root()?
                    .ok_or_else(|| error::Error::Message("请先设置统一目录".into()))?;
                let prepared = network::prepare_catalog(
                    &root.join("cache"),
                    text(&request, "slug")?,
                    optional(&request, "site", "clawhub"),
                )
                .await?;
                let info = prepared_info(&prepared);
                let selected = if info.scan_subdir.is_empty() {
                    vec![]
                } else {
                    let paths: Vec<_> = files::scan(&prepared.path.join(&info.scan_subdir))?
                        .items
                        .into_iter()
                        .filter(|i| i.status == "ready")
                        .map(|i| i.path)
                        .collect();
                    if paths.is_empty() {
                        return fail("来源子目录中未发现 Skill");
                    }
                    paths
                };
                let state = self.import_folder(&prepared.path, selected, false, Some(info))?;
                Ok(serde_json::to_value(state)?)
            }
            "check_source" => Ok(serde_json::to_value(
                self.check_source(text(&request, "sourceId")?, flag(&request, "apply"))
                    .await?,
            )?),
            "retry_backup_cleanup" => Ok(serde_json::to_value(self.transact(
                "backup_cleanup",
                "重试旧备份清理",
                |_, _, _| Ok(()),
            )?)?),
            "list_backups" => Ok(serde_json::to_value(self.list_backups()?)?),
            "preview_object_cleanup" => Ok(serde_json::to_value(self.preview_object_cleanup()?)?),
            "list_object_cleanups" => Ok(serde_json::to_value(self.list_object_cleanups()?)?),
            "cleanup_objects" => Ok(serde_json::to_value(
                self.cleanup_objects(
                    serde_json::from_value(
                        request
                            .get("expectedRevision")
                            .cloned()
                            .unwrap_or(Value::Null),
                    )?,
                    serde_json::from_value(request.get("items").cloned().unwrap_or(Value::Null))?,
                )?,
            )?),
            "retry_object_cleanup" => Ok(serde_json::to_value(
                self.retry_object_cleanup(text(&request, "taskId")?)?,
            )?),
            "preview_restore" => Ok(serde_json::to_value(
                self.preview_restore(text(&request, "backupId")?)?,
            )?),
            "restore_backup" => Ok(serde_json::to_value(
                self.restore_backup_cleanup(
                    text(&request, "backupId")?,
                    request
                        .get("expectedRevision")
                        .map(|value| serde_json::from_value::<u32>(value.clone()))
                        .transpose()?,
                    serde_json::from_value(
                        request.get("cleanupItems").cloned().unwrap_or(json!([])),
                    )?,
                )?,
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
        updates_removed: None,
        local_member_ids: None,
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

#[cfg(test)]
mod discovery_display_tests {
    use super::*;

    #[test]
    fn shared_directory_remains_visible_under_each_profile() {
        let temp = tempfile::tempdir().unwrap();
        let shared = temp.path().join("shared");
        let specific = temp.path().join("specific");
        fs::create_dir_all(shared.join("example")).unwrap();
        fs::create_dir_all(&specific).unwrap();
        fs::write(shared.join("example/SKILL.md"), "# Shared skill").unwrap();
        let shared = shared.display().to_string();
        let profiles = vec![
            AgentProfile {
                id: "agents".into(),
                name: "Shared".into(),
                user_paths: vec![shared.clone()],
                project_paths: vec![],
            },
            AgentProfile {
                id: "codex".into(),
                name: "Codex".into(),
                user_paths: vec![shared, specific.display().to_string()],
                project_paths: vec![],
            },
        ];
        let targets = Engine::discover_profiles(&profiles).unwrap();
        assert_eq!(
            targets
                .iter()
                .filter(|target| target.tool == "codex")
                .count(),
            2
        );
        assert_eq!(targets.len(), 3);
        let paths = targets
            .into_iter()
            .map(|target| target.path)
            .collect::<Vec<_>>();
        assert_eq!(files::scan_many(&paths).unwrap().items.len(), 1);
    }
}
