use super::*;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupItem {
    pub digest: String,
    pub path: String,
    pub status: String,
    pub reason: String,
    pub fingerprint: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestorePreview {
    pub revision: u32,
    pub items: Vec<CleanupItem>,
}

fn external_reference_roots(state: &Snapshot) -> Vec<&String> {
    state
        .skills
        .iter()
        .chain(
            state
                .presets
                .iter()
                .flat_map(|preset| preset.locks.values()),
        )
        .filter_map(|skill| skill.external_path.as_ref())
        .chain(state.unmanaged_target_paths.iter())
        .chain(
            state
                .bindings
                .iter()
                .filter_map(|binding| binding.external_path.as_ref()),
        )
        .collect()
}

impl Engine {
    pub(crate) fn check_external_migration(
        &self,
        root: &Path,
        state: &Snapshot,
        new_root: &Path,
    ) -> Result<()> {
        for path in state.packages.iter().map(|p| &p.path).chain(
            state
                .sources
                .iter()
                .filter(|s| s.kind == "local")
                .map(|s| &s.path),
        ) {
            let actual = fs::canonicalize(path).unwrap_or_else(|_| PathBuf::from(path));
            if new_root.starts_with(&actual) || actual.starts_with(new_root) {
                return fail("新统一目录不能与包或本地来源互相包含");
            }
        }
        let paths = external_reference_roots(state);
        if paths.is_empty() {
            return Ok(());
        }
        if paths
            .iter()
            .any(|path| Path::new(path).starts_with(root) || root.starts_with(path))
        {
            return fail("本地引用位置与当前统一库互相包含，拒绝迁移并移除旧库");
        }
        if paths
            .iter()
            .any(|path| Path::new(path).starts_with(new_root) || new_root.starts_with(path))
        {
            return fail("新统一目录不能与本地引用实体互相包含");
        }
        let mut scan = state.clone();
        scan.targets.clear();
        scan.settings.agent_profiles.clear();
        scan.sources
            .retain(|source| source.kind == "local_reference");
        let candidates = fs::read_dir(root.join("objects"))?
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name.len() == 64 && name.bytes().all(|b| b.is_ascii_hexdigit()))
            .collect();
        let mut used = BTreeMap::new();
        Self::filesystem_cleanup_references(
            root,
            &scan,
            &[],
            &candidates,
            None,
            &mut used,
            100_000,
        )?;
        if !used.is_empty() {
            return fail(
                "本地引用或已移除目标的保留链接仍指向当前统一库，请先调整这些依赖再迁移，避免旧库移除后失效",
            );
        }
        Ok(())
    }

    fn all_journals(&self, root: &Path) -> Result<Vec<Journal>> {
        let mut journals = vec![];
        for entry in fs::read_dir(root.join("transactions"))? {
            let path = entry?.path();
            if path.extension().and_then(|p| p.to_str()) == Some("json") {
                journals.push(serde_json::from_slice(&fs::read(path)?)?);
            }
        }
        Ok(journals)
    }

    pub(crate) fn cleanup_references(
        &self,
        root: &Path,
        state: &Snapshot,
        ignore: Option<&str>,
    ) -> Result<BTreeMap<String, String>> {
        let mut used = BTreeMap::new();
        for skill in &state.skills {
            used.insert(
                skill.bundle_digest.clone(),
                format!("Skill「{}」仍在使用", skill.name),
            );
        }
        for binding in &state.bindings {
            used.insert(
                binding.digest.clone(),
                format!("分发目录仍在使用：{}", binding.path),
            );
        }
        for preset in &state.presets {
            for skill in preset.locks.values() {
                used.insert(
                    skill.bundle_digest.clone(),
                    format!("预设「{}」锁定此版本", preset.name),
                );
            }
        }
        for source in &state.sources {
            if state.schema_version >= 3 && !state.skills.iter().any(|s| s.source_id == source.id) {
                continue;
            }
            used.entry(source.version.clone())
                .or_insert_with(|| format!("来源「{}」仍在使用", source.name));
        }
        for backup in &state.content_backups {
            for skill in &backup.skills {
                used.insert(skill.bundle_digest.clone(), "最近一次更新的恢复备份".into());
            }
        }
        for journal in self.all_journals(root)? {
            if ignore == Some(journal.id.as_str()) {
                continue;
            }
            if Self::cleanup_committed(&journal, state) {
                if let Some(job) = &journal.object_cleanup {
                    for item in &job.items {
                        if matches!(item.status.as_str(), "pending" | "failed") {
                            used.entry(item.choice.digest.clone())
                                .or_insert_with(|| "其他已确认的清理任务仍在处理此实体".into());
                        }
                    }
                }
            }
            let unsettled = matches!(journal.status.as_str(), "running" | "needsRecovery")
                && !state
                    .tasks
                    .iter()
                    .any(|task| task.id == journal.id && task.status == "success");
            for old in journal.before.skills.iter().chain(&journal.after.skills) {
                if unsettled
                    || (state.schema_version < 3
                        && state.skills.iter().any(|skill| skill.id == old.id))
                {
                    used.entry(old.bundle_digest.clone()).or_insert_with(|| {
                        if unsettled {
                            "未完成事务仍需要此版本".into()
                        } else {
                            format!("Skill「{}」的历史版本仍可回滚", old.name)
                        }
                    });
                }
            }
            if unsettled {
                for b in journal
                    .before
                    .bindings
                    .iter()
                    .chain(&journal.after.bindings)
                {
                    used.insert(b.digest.clone(), "未完成事务的分发仍需要此版本".into());
                }
                for p in journal.before.presets.iter().chain(&journal.after.presets) {
                    for s in p.locks.values() {
                        used.insert(
                            s.bundle_digest.clone(),
                            "未完成事务的预设仍需要此版本".into(),
                        );
                    }
                }
            }
            let restored = journal.status == "restored"
                || state.tasks.iter().any(|t| {
                    t.kind == "restore_backup" && t.status == "success" && t.message == journal.id
                });
            if !restored
                && matches!(
                    journal.status.as_str(),
                    "committed" | "pruning" | "running" | "needsRecovery"
                )
            {
                for c in journal.changes.iter().filter(|c| {
                    c.backup
                        .as_ref()
                        .is_some_and(|p| state.schema_version < 3 || p.exists())
                        && !c.restore
                }) {
                    if let Some(binding) = journal
                        .after
                        .bindings
                        .iter()
                        .find(|b| Path::new(&b.path) == c.path)
                    {
                        used.entry(binding.digest.clone())
                            .or_insert_with(|| "其他归集备份仍需要此版本".into());
                    }
                }
            }
        }
        Ok(used)
    }

    pub(crate) fn object_path(root: &Path, digest: &str) -> Result<PathBuf> {
        if digest.len() != 64 || !digest.bytes().all(|b| b.is_ascii_hexdigit()) {
            return fail("无效副本标识");
        }
        let path = root.join("objects").join(digest);
        if files::exists(&path) && fs::canonicalize(&path)? != path {
            return fail("实体路径被替换或指向其他目录");
        }
        Ok(path)
    }

    fn inspect_cleanup_item(
        &self,
        root: &Path,
        digest: &str,
        used: &BTreeMap<String, String>,
    ) -> CleanupItem {
        let mut item = CleanupItem {
            digest: digest.into(),
            path: root.join("objects").join(digest).display().to_string(),
            status: "blocked".into(),
            reason: String::new(),
            fingerprint: String::new(),
        };
        if let Some(reason) = used.get(digest) {
            item.status = "referenced".into();
            item.reason = reason.clone();
            return item;
        }
        let checked = (|| -> Result<()> {
            let path = Self::object_path(root, digest)?;
            if !files::exists(&path) {
                item.status = "missing".into();
                item.reason = "实体已不存在".into();
                return Ok(());
            }
            item.fingerprint = files::manifest_digest(&path)?;
            let only_tree =
                fs::read_dir(&path)?.all(|entry| entry.is_ok_and(|e| e.file_name() == "tree"));
            let unchanged =
                only_tree && files::snapshot_matches(&path.join("tree"), digest).unwrap_or(false);
            item.status = if unchanged { "ready" } else { "modified" }.into();
            item.reason = if unchanged {
                "管理记录和已配置目录内未发现引用，内容未修改，可选择永久清理"
            } else {
                "实体内容已修改，默认保留；删除会丢弃归集后的修改"
            }
            .into();
            Ok(())
        })();
        if let Err(error) = checked {
            item.reason = format!("无法校验，保留：{error}");
        }
        item
    }

    // Read-only reachability check. Follow directory aliases with canonical
    // deduplication, but never traverse the library itself as a reference root.
    // A bounded/incomplete scan must not produce a "ready" candidate.
    fn filesystem_cleanup_references(
        root: &Path,
        state: &Snapshot,
        changes: &[Change],
        candidates: &BTreeSet<String>,
        staging_task: Option<&str>,
        used: &mut BTreeMap<String, String>,
        max_entries: usize,
    ) -> Result<()> {
        fn entry_identity(path: &Path) -> std::io::Result<PathBuf> {
            match (path.parent(), path.file_name()) {
                (Some(parent), Some(name)) => Ok(fs::canonicalize(parent)?.join(name)),
                _ => fs::canonicalize(path),
            }
        }
        let restored = changes
            .iter()
            .filter(|c| c.restore)
            .map(|c| entry_identity(&c.path))
            .collect::<std::io::Result<BTreeSet<_>>>()?;
        let mut pending = BTreeSet::new();
        for path in state
            .targets
            .iter()
            .map(|t| &t.path)
            .chain(
                state
                    .settings
                    .agent_profiles
                    .iter()
                    .flat_map(|p| &p.user_paths),
            )
            .chain(
                state
                    .sources
                    .iter()
                    .filter(|s| matches!(s.kind.as_str(), "local" | "local_reference"))
                    .map(|s| &s.path),
            )
            .chain(external_reference_roots(state))
        {
            pending.insert(files::absolute(path)?);
        }
        for change in changes {
            if let Some(parent) = change.path.parent() {
                pending.insert(parent.to_path_buf());
            }
        }
        let mut objects: Vec<_> = candidates
            .iter()
            .map(|digest| (digest, root.join("objects").join(digest)))
            .collect();
        if let Some(task_id) = staging_task {
            objects.extend(candidates.iter().map(|digest| {
                (
                    digest,
                    root.join(".object-cleanup").join(task_id).join(digest),
                )
            }));
        }
        let mut visited = BTreeSet::new();
        let mut examined = 0;
        while let Some(path) = pending.pop_first() {
            examined += 1;
            if examined > max_entries {
                return fail("软链检查超过扫描上限，无法确认全部引用");
            }
            let metadata = match fs::symlink_metadata(&path) {
                Ok(metadata) => metadata,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(e) => return fail(format!("无法检查 {}：{e}", path.display())),
            };
            // Only these exact directory entries disappear during this restore.
            // Other aliases to the same object must remain protected.
            if restored.contains(&entry_identity(&path)?) {
                continue;
            }
            // The entry existed above. Even NotFound now means a dangling link
            // or a concurrent filesystem change, not a completed reference scan.
            let resolved = fs::canonicalize(&path)
                .map_err(|e| error::Error::Message(format!("无法解析 {}：{e}", path.display())))?;
            // Explicit external roots can point directly inside an object;
            // preserve them even after the original installation link is gone.
            for (digest, object) in &objects {
                if !metadata.file_type().is_symlink() && resolved.starts_with(object) {
                    used.entry((*digest).clone())
                        .or_insert_with(|| format!("外部位置仍引用此实体：{}", path.display()));
                }
            }
            if metadata.file_type().is_symlink() {
                for (digest, object) in &objects {
                    if resolved.starts_with(object) || object.starts_with(&resolved) {
                        used.entry((*digest).clone())
                            .or_insert_with(|| format!("目录中仍有软链引用：{}", path.display()));
                    }
                }
            }
            if resolved.starts_with(root) || !resolved.is_dir() || !visited.insert(resolved.clone())
            {
                continue;
            }
            for entry in fs::read_dir(&resolved).map_err(|e| {
                error::Error::Message(format!("无法读取 {}：{e}", resolved.display()))
            })? {
                pending.insert(entry?.path());
                if pending.len() + examined > max_entries {
                    return fail("软链检查超过扫描上限，无法确认全部引用");
                }
            }
        }
        Ok(())
    }

    pub(crate) fn plan_restore_cleanup(
        &self,
        root: &Path,
        before: &Snapshot,
        after: &Snapshot,
        changes: &[Change],
    ) -> Result<RestorePreview> {
        let journals = self.all_journals(root)?;
        let mut affected = BTreeSet::new();
        for marker in after.tasks.iter().filter(|t| {
            t.kind == "restore_backup" && !before.tasks.iter().any(|old| old.id == t.id)
        }) {
            let original = journals
                .iter()
                .find(|j| j.id == marker.message)
                .ok_or_else(|| error::Error::Message("缺少原归集记录".into()))?;
            affected.extend(
                original
                    .after
                    .skills
                    .iter()
                    .filter(|s| !original.before.skills.iter().any(|old| old.id == s.id))
                    .map(|s| s.id.clone()),
            );
        }
        let mut candidates = BTreeSet::new();
        for skill in before.skills.iter().chain(&after.skills).chain(
            journals
                .iter()
                .flat_map(|j| j.before.skills.iter().chain(&j.after.skills)),
        ) {
            if affected.contains(&skill.id) && skill.external_path.is_none() {
                candidates.insert(skill.bundle_digest.clone());
            }
        }
        self.preview_cleanup_candidates(root, before, after, changes, candidates)
    }
    fn preview_cleanup_candidates(
        &self,
        root: &Path,
        before: &Snapshot,
        after: &Snapshot,
        changes: &[Change],
        candidates: BTreeSet<String>,
    ) -> Result<RestorePreview> {
        let mut used = self.cleanup_references(root, after, None)?;
        let scan_error = if candidates.is_empty() {
            None
        } else {
            Self::filesystem_cleanup_references(
                root,
                before,
                changes,
                &candidates,
                None,
                &mut used,
                50_000,
            )
            .err()
            .map(|e| e.to_string())
        };
        Ok(RestorePreview {
            revision: before.revision,
            items: candidates
                .iter()
                .map(|digest| {
                    let mut item = self.inspect_cleanup_item(root, digest, &used);
                    if item.status == "ready" {
                        if let Some(error) = &scan_error {
                            item.status = "blocked".into();
                            item.reason = format!("无法完成引用检查，保留：{error}");
                        }
                    }
                    item
                })
                .collect(),
        })
    }

    pub fn preview_restore(&self, backup_id: &str) -> Result<RestorePreview> {
        let _guard = self.lock()?;
        let root = self
            .root()?
            .ok_or_else(|| error::Error::Message("尚未初始化".into()))?;
        let before = self.snapshot()?;
        let mut after = before.clone();
        let mut changes = vec![];
        self.prepare_restore(&mut after, &root, &mut changes, backup_id)?;
        let plan = self.plan_restore_cleanup(&root, &before, &after, &changes)?;
        Ok(RestorePreview {
            revision: before.revision,
            items: plan.items,
        })
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupChoice {
    pub digest: String,
    pub fingerprint: String,
}
#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct CleanupWork {
    choice: CleanupChoice,
    phase: String,
    status: String,
    reason: String,
    inventory: Option<Vec<files::TreeEntry>>,
}
#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct ObjectCleanup {
    items: Vec<CleanupWork>,
    scan_roots: Vec<String>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupReport {
    id: String,
    status: String,
    items: Vec<CleanupItem>,
}

impl Engine {
    fn scan_roots(state: &Snapshot, changes: &[Change]) -> Vec<String> {
        let mut paths: BTreeSet<_> = state
            .targets
            .iter()
            .map(|t| t.path.clone())
            .chain(
                state
                    .settings
                    .agent_profiles
                    .iter()
                    .flat_map(|p| p.user_paths.clone()),
            )
            .chain(
                state
                    .sources
                    .iter()
                    .filter(|s| matches!(s.kind.as_str(), "local" | "local_reference"))
                    .map(|s| s.path.clone()),
            )
            .collect();
        paths.extend(external_reference_roots(state).into_iter().cloned());
        paths.extend(
            changes
                .iter()
                .filter_map(|c| c.path.parent())
                .map(|p| p.display().to_string()),
        );
        paths.into_iter().collect()
    }
    fn with_scan_roots(state: &Snapshot, paths: &[String]) -> Snapshot {
        let mut scan = state.clone();
        scan.targets.extend(paths.iter().map(|path| Target {
            id: String::new(),
            name: String::new(),
            tool: "custom".into(),
            scope: "user".into(),
            path: path.clone(),
        }));
        scan
    }
    pub(crate) fn prepare_object_cleanup(
        &self,
        _root: &Path,
        before: &Snapshot,
        changes: &[Change],
        plan: &RestorePreview,
        choices: Vec<CleanupChoice>,
    ) -> Result<ObjectCleanup> {
        let mut seen = BTreeSet::new();
        let mut items = vec![];
        for choice in choices {
            if !seen.insert(choice.digest.clone()) {
                return fail("清理选择重复");
            }
            if !plan.items.iter().any(|p| {
                p.digest == choice.digest
                    && p.status == "ready"
                    && !choice.fingerprint.is_empty()
                    && p.fingerprint == choice.fingerprint
            }) {
                return fail("清理条件或文件内容已变化，请重新预览");
            }
            items.push(CleanupWork {
                choice,
                phase: "pending".into(),
                status: "pending".into(),
                reason: "等待已确认的清理任务执行".into(),
                inventory: None,
            });
        }
        if items.is_empty() {
            return fail("请至少选择一个可清理实体");
        }
        Ok(ObjectCleanup {
            items,
            scan_roots: Self::scan_roots(before, changes),
        })
    }
    pub(crate) fn history_cleanup_plan(
        &self,
        root: &Path,
        state: &Snapshot,
    ) -> Result<(RestorePreview, Snapshot)> {
        let mut historical = BTreeSet::new();
        for journal in self.all_journals(root)? {
            historical.extend(Self::scan_roots(&journal.before, &journal.changes));
            historical.extend(Self::scan_roots(&journal.after, &journal.changes));
        }
        let scan = Self::with_scan_roots(state, &historical.into_iter().collect::<Vec<_>>());
        let mut candidates = BTreeSet::new();
        for entry in fs::read_dir(root.join("objects"))? {
            let name = entry?.file_name().to_string_lossy().into_owned();
            if name.len() == 64 && name.bytes().all(|b| b.is_ascii_hexdigit()) {
                candidates.insert(name);
            }
        }
        let plan = self.preview_cleanup_candidates(root, &scan, state, &[], candidates)?;
        Ok((plan, scan))
    }
    pub fn preview_object_cleanup(&self) -> Result<RestorePreview> {
        let _guard = self.lock()?;
        let root = self
            .root()?
            .ok_or_else(|| error::Error::Message("尚未初始化".into()))?;
        let state = self.snapshot()?;
        Ok(self.history_cleanup_plan(&root, &state)?.0)
    }
    pub fn cleanup_objects(&self, revision: u32, choices: Vec<CleanupChoice>) -> Result<Snapshot> {
        self.transact_with_cleanup(
            "object_cleanup",
            "清理统一库历史残留",
            |state, root, changes, job| {
                if state.revision != revision {
                    return fail("状态已变化，请重新预览清理内容");
                }
                let (plan, scan) = self.history_cleanup_plan(root, state)?;
                *job = Some(self.prepare_object_cleanup(root, &scan, changes, &plan, choices)?);
                Ok(())
            },
        )
    }
    fn cleanup_committed(journal: &Journal, state: &Snapshot) -> bool {
        matches!(journal.status.as_str(), "running" | "committed")
            && state
                .tasks
                .iter()
                .any(|t| t.id == journal.id && t.status == "success")
    }
    fn cleanup_report(root: &Path, journal: &Journal) -> CleanupReport {
        let job = journal.object_cleanup.as_ref().unwrap();
        let pending = job
            .items
            .iter()
            .any(|i| matches!(i.status.as_str(), "pending" | "failed"));
        CleanupReport {
            id: journal.id.clone(),
            status: if pending { "pending" } else { "complete" }.into(),
            items: job
                .items
                .iter()
                .map(|i| {
                    let staged = root
                        .join(".object-cleanup")
                        .join(&journal.id)
                        .join(&i.choice.digest);
                    let path = if files::exists(&staged) {
                        staged
                    } else {
                        root.join("objects").join(&i.choice.digest)
                    };
                    CleanupItem {
                        digest: i.choice.digest.clone(),
                        path: path.display().to_string(),
                        status: i.status.clone(),
                        reason: i.reason.clone(),
                        fingerprint: i.choice.fingerprint.clone(),
                    }
                })
                .collect(),
        }
    }
    pub fn list_object_cleanups(&self) -> Result<Vec<CleanupReport>> {
        let Some(root) = self.root()? else {
            return Ok(vec![]);
        };
        let state = self.snapshot()?;
        Ok(self
            .all_journals(&root)?
            .iter()
            .filter(|j| j.object_cleanup.is_some() && Self::cleanup_committed(j, &state))
            .map(|j| Self::cleanup_report(&root, j))
            .collect())
    }
    pub(crate) fn decorate_object_cleanups(&self, root: &Path, state: &mut Snapshot) -> Result<()> {
        for journal in self.all_journals(root)? {
            if journal.object_cleanup.is_none() || !Self::cleanup_committed(&journal, state) {
                continue;
            }
            let report = Self::cleanup_report(root, &journal);
            let task_id = format!("object-cleanup:{}", journal.id);
            state.tasks.retain(|t| t.id != task_id);
            let count = |status: &str| report.items.iter().filter(|i| i.status == status).count();
            state.tasks.push(Task {
                id: task_id,
                kind: "object_cleanup_result".into(),
                title: "统一库实体清理".into(),
                status: if report.status == "complete" {
                    "success"
                } else {
                    "failed"
                }
                .into(),
                message: format!(
                    "已清理 {}，已保留 {}，待重试 {}",
                    count("removed"),
                    count("retained"),
                    count("pending") + count("failed")
                ),
                created_at: journal
                    .after
                    .tasks
                    .iter()
                    .find(|t| t.id == journal.id)
                    .map(|t| t.created_at.clone())
                    .unwrap_or_default(),
            });
        }
        Ok(())
    }
    fn cleanup_staging(root: &Path, task_id: &str) -> Result<PathBuf> {
        uuid::Uuid::parse_str(task_id).map_err(|_| error::Error::Message("无效清理任务".into()))?;
        let mut path = root.to_path_buf();
        for component in [".object-cleanup", task_id] {
            path.push(component);
            match fs::create_dir(&path) {
                Ok(()) => (),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
                Err(e) => return Err(e.into()),
            }
            if fs::canonicalize(&path)? != path || !fs::symlink_metadata(&path)?.is_dir() {
                return fail("清理暂存目录被替换，已停止");
            }
        }
        Ok(path)
    }
    fn remove_object_item(
        root: &Path,
        task_id: &str,
        work: &mut CleanupWork,
        mut persist: impl FnMut(&CleanupWork) -> Result<()>,
    ) -> Result<()> {
        let original = Self::object_path(root, &work.choice.digest)?;
        let stage = Self::cleanup_staging(root, task_id)?.join(&work.choice.digest);
        if files::exists(&stage) && fs::canonicalize(&stage)? != stage {
            return fail("清理实体被替换为软链");
        }
        if work.phase == "staging" && !files::exists(&stage) {
            work.status = "retained".into();
            work.reason =
                "暂存阶段被中断，无法确认原位置是否为新实体；已保留，可重新预览清理".into();
            return Ok(());
        }
        if work.phase == "pending" {
            if !files::exists(&original) {
                work.status = "removed".into();
                work.phase = "done".into();
                work.reason = "实体已不存在".into();
                return Ok(());
            }
            if files::manifest_digest(&original)? != work.choice.fingerprint {
                work.status = "retained".into();
                work.reason = "确认后实体内容发生变化，已保留".into();
                return Ok(());
            }
            if files::exists(&stage) {
                return fail("清理暂存位置被占用");
            }
            work.inventory = Some(files::tree_manifest(&original, false)?);
            work.phase = "staging".into();
            persist(work)?; // Durable intent before rename; restart checks both locations.
            if let Err(error) = fs::rename(&original, &stage) {
                // A returned rename error is distinguishable from a crash in the
                // staging window; keep the ordinary permission failure retryable.
                work.phase = "pending".into();
                work.inventory = None;
                persist(work)?;
                return Err(error.into());
            }
        }
        if !files::exists(&stage) && work.phase == "deleting" {
            // A new object with the same digest may now exist: never touch it.
            work.status = "removed".into();
            work.phase = "done".into();
            work.reason = "实体已清理".into();
            return Ok(());
        }
        if work.phase == "staging" {
            if files::manifest_digest(&stage)? != work.choice.fingerprint {
                // No deletion has begun, so restore modified content where possible.
                if !files::exists(&original) {
                    fs::rename(&stage, &original)?;
                }
                work.status = "retained".into();
                work.reason = "移入清理暂存目录时内容变化，已保留".into();
                return Ok(());
            }
            work.phase = "deleting".into();
            persist(work)?;
        }
        let inventory = work
            .inventory
            .as_ref()
            .ok_or_else(|| error::Error::Message("缺少持久化删除清单".into()))?;
        let expected: BTreeMap<_, _> = inventory
            .iter()
            .map(|entry| (entry.relative.as_str(), entry))
            .collect();
        for entry in files::tree_manifest(&stage, false)? {
            if expected.get(entry.relative.as_str()).copied() != Some(&entry) {
                work.status = "retained".into();
                work.reason = "清理暂存内容新增或变化，剩余文件已保留".into();
                return Ok(());
            }
        }
        for entry in inventory.iter().rev() {
            let relative = files::safe_relative(&entry.relative)?;
            if relative.as_os_str().is_empty() {
                return fail("删除清单含无效根路径");
            }
            let path = stage.join(relative);
            match fs::symlink_metadata(&path) {
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(e) => return Err(e.into()),
                Ok(_) => (),
            }
            if fs::canonicalize(path.parent().unwrap())? != path.parent().unwrap()
                || files::manifest_entry(&path, entry.relative.clone())? != *entry
            {
                return fail("删除前目录或文件已变化，剩余内容已保留");
            }
            match entry.kind.as_str() {
                "directory" => fs::remove_dir(&path)?,
                "link" => files::remove_link(&path)?,
                "file" => fs::remove_file(&path)?,
                _ => return fail("删除清单包含不支持的条目"),
            }
        }
        fs::remove_dir(&stage)?;
        work.phase = "done".into();
        work.status = "removed".into();
        work.reason = "实体已永久清理".into();
        Ok(())
    }
    pub(crate) fn run_object_cleanup(
        &self,
        root: &Path,
        state: &Snapshot,
        journal: &mut Journal,
    ) -> Result<()> {
        if !Self::cleanup_committed(journal, state) {
            return fail("原事务尚未成功提交，不能清理实体");
        }
        let job = journal
            .object_cleanup
            .as_ref()
            .ok_or_else(|| error::Error::Message("该任务没有实体清理计划".into()))?
            .clone();
        let log = root
            .join("transactions")
            .join(format!("{}.json", journal.id));
        let scan = Self::with_scan_roots(state, &job.scan_roots);
        for (index, mut work) in job.items.into_iter().enumerate() {
            if !matches!(work.status.as_str(), "pending" | "failed") {
                continue;
            }
            let result = (|| -> Result<()> {
                let mut used = self.cleanup_references(root, state, Some(&journal.id))?;
                Self::filesystem_cleanup_references(
                    root,
                    &scan,
                    &[],
                    &BTreeSet::from([work.choice.digest.clone()]),
                    Some(&journal.id),
                    &mut used,
                    50_000,
                )?;
                if let Some(reason) = used.get(&work.choice.digest) {
                    work.status = "retained".into();
                    work.reason = reason.clone();
                    return Ok(());
                }
                let task_id = journal.id.clone();
                Self::remove_object_item(root, &task_id, &mut work, |progress| {
                    journal.object_cleanup.as_mut().unwrap().items[index] = progress.clone();
                    files::atomic_json(&log, journal)
                })
            })();
            if let Err(error) = result {
                work.status = "failed".into();
                work.reason = error.to_string();
            }
            journal.object_cleanup.as_mut().unwrap().items[index] = work;
            files::atomic_json(&log, journal)?;
        }
        // Only remove empty staging containers. Retained/partial files stay visible in the report.
        let _ = fs::remove_dir(root.join(".object-cleanup").join(&journal.id));
        let _ = fs::remove_dir(root.join(".object-cleanup"));
        Ok(())
    }
    pub fn retry_object_cleanup(&self, task_id: &str) -> Result<Snapshot> {
        let task_id = uuid::Uuid::parse_str(task_id)
            .map_err(|_| error::Error::Message("无效清理任务 ID".into()))?
            .to_string();
        let _guard = self.lock()?;
        let root = self
            .root()?
            .ok_or_else(|| error::Error::Message("尚未初始化".into()))?;
        let library_lock = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(root.join("manager.lock"))?;
        fs4::FileExt::try_lock(&library_lock)
            .map_err(|_| error::Error::Message("中央库正在使用".into()))?;
        let mut state = self.snapshot()?;
        if state.tasks.iter().any(|t| t.status == "needsRecovery") {
            return fail("请先恢复未完成的文件事务");
        }
        let mut journal: Journal = serde_json::from_slice(&fs::read(
            root.join("transactions").join(format!("{task_id}.json")),
        )?)?;
        if journal.id != task_id
            || !Self::cleanup_committed(&journal, &state)
            || journal.object_cleanup.is_none()
        {
            return fail("该任务没有已提交的清理计划");
        }
        state.revision = state
            .revision
            .checked_add(1)
            .ok_or_else(|| error::Error::Message("版本计数已满".into()))?;
        files::atomic_json(&root.join("state.json"), &state)?;
        self.run_object_cleanup(&root, &state, &mut journal)?;
        self.snapshot()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn setup() -> (tempfile::TempDir, Engine, PathBuf, String) {
        let temp = tempfile::tempdir().unwrap();
        let base = fs::canonicalize(temp.path()).unwrap();
        let engine = Engine::new(Some(base.join("config"))).unwrap();
        engine
            .configure_legacy_fixture(base.join("library").to_str().unwrap())
            .unwrap();
        engine
            .transact("test", "isolate configured scan roots", |state, _, _| {
                state.settings.agent_profiles.clear();
                Ok(())
            })
            .unwrap();
        let skill = base.join("skill");
        fs::create_dir(&skill).unwrap();
        fs::write(skill.join("SKILL.md"), "# Original").unwrap();
        let state = engine.import_folder(&skill, vec![], true, None).unwrap();
        let backup = state.tasks.last().unwrap().id.clone();
        (temp, engine, skill, backup)
    }
    #[test]
    fn preview_includes_intermediate_versions_and_keeps_modified_content() {
        let (temp, engine, original, backup) = setup();
        let root = engine.root().unwrap().unwrap();
        let mut versions = vec![engine.snapshot().unwrap().skills[0].bundle_digest.clone()];
        for n in 1..=2 {
            let next = temp.path().join(format!("version-{n}"));
            fs::create_dir(&next).unwrap();
            fs::write(next.join("SKILL.md"), format!("# Version {n}")).unwrap();
            let digest = files::snapshot_tree(&root, &next).unwrap();
            engine
                .transact("update", "test version", |state, _, _| {
                    state.skills[0].bundle_digest = digest.clone();
                    state.sources[0].version = digest.clone();
                    Ok(())
                })
                .unwrap();
            versions.push(digest);
        }
        fs::write(
            root.join("objects")
                .join(&versions[1])
                .join("tree/notes.txt"),
            "user changes",
        )
        .unwrap();
        let state_bytes = fs::read(root.join("state.json")).unwrap();
        let preview = engine.preview_restore(&backup).unwrap();
        assert_eq!(preview.items.len(), 3);
        assert_eq!(
            preview
                .items
                .iter()
                .find(|i| i.digest == versions[1])
                .unwrap()
                .status,
            "modified"
        );
        assert_eq!(
            preview.items.iter().filter(|i| i.status == "ready").count(),
            2
        );
        assert_eq!(fs::read(root.join("state.json")).unwrap(), state_bytes);
        assert!(fs::read_link(original).is_ok());
        for version in versions {
            assert!(root.join("objects").join(version).is_dir());
        }
    }
    #[test]
    fn preview_preserves_versions_needed_by_an_unfinished_transaction() {
        let (_temp, engine, _original, backup) = setup();
        let root = engine.root().unwrap().unwrap();
        let mut other: Journal = serde_json::from_slice(
            &fs::read(root.join("transactions").join(format!("{backup}.json"))).unwrap(),
        )
        .unwrap();
        other.id = id();
        other.status = "running".into();
        other.changes.clear();
        files::atomic_json(
            &root.join("transactions").join(format!("{}.json", other.id)),
            &other,
        )
        .unwrap();
        let preview = engine.preview_restore(&backup).unwrap();
        assert_eq!(preview.items[0].status, "referenced");
        assert!(preview.items[0].reason.contains("未完成"));
    }
    #[test]
    fn stale_preview_cannot_restore_after_state_changes() {
        let (_temp, engine, original, backup) = setup();
        let preview = engine.preview_restore(&backup).unwrap();
        engine
            .execute_local("settings", &json!({"backupRetention": 4}))
            .unwrap();
        assert!(
            engine
                .restore_backup_options(&backup, Some(preview.revision))
                .is_err()
        );
        assert!(fs::read_link(original).is_ok());
        assert_eq!(engine.snapshot().unwrap().skills.len(), 1);
    }
    #[test]
    #[cfg(unix)]
    fn preview_finds_unregistered_links_in_every_configured_root_kind() {
        use std::os::unix::fs::symlink;
        for root_kind in ["target", "profile", "source"] {
            let (_temp, engine, original, backup) = setup();
            let external = tempfile::tempdir().unwrap();
            let external_root = fs::canonicalize(external.path()).unwrap();
            let hidden = external_root.join(".system/nested");
            fs::create_dir_all(&hidden).unwrap();
            let state = engine.snapshot().unwrap();
            let object = skill_path(Path::new(&state.storage_root), &state.skills[0]);
            symlink(object.join("SKILL.md"), external_root.join("first")).unwrap();
            symlink("../../first", hidden.join("relative-chain")).unwrap();
            let alias = external_root.join("alias");
            symlink(external_root.join(".system"), &alias).unwrap();
            engine
                .transact("test", "configured reference", |state, _, _| {
                    let path = alias.display().to_string();
                    match root_kind {
                        "target" => state.targets.push(Target {
                            id: id(),
                            name: "External".into(),
                            tool: "custom".into(),
                            scope: "project".into(),
                            path,
                        }),
                        "profile" => state.settings.agent_profiles.push(AgentProfile {
                            id: id(),
                            name: "External".into(),
                            user_paths: vec![path],
                            project_paths: vec![],
                        }),
                        _ => {
                            let mut source = state.sources[0].clone();
                            source.id = id();
                            source.path = path;
                            source.version.clear();
                            state.sources.push(source);
                        }
                    }
                    Ok(())
                })
                .unwrap();
            let before = fs::read(Path::new(&state.storage_root).join("state.json")).unwrap();
            let preview = engine.preview_restore(&backup).unwrap();
            assert_eq!(preview.items[0].status, "referenced", "{root_kind}");
            assert!(preview.items[0].reason.contains("软链引用"));
            assert_eq!(
                fs::read(Path::new(&state.storage_root).join("state.json")).unwrap(),
                before
            );
            assert!(fs::read_link(&original).is_ok());
            assert!(object.join("SKILL.md").is_file());
        }
    }

    #[test]
    #[cfg(unix)]
    fn preview_blocks_candidates_when_link_resolution_fails() {
        for destination in ["unresolvable", "missing-destination"] {
            let (temp, engine, _, backup) = setup();
            std::os::unix::fs::symlink(destination, temp.path().join("unresolvable")).unwrap();
            let preview = engine.preview_restore(&backup).unwrap();
            assert_eq!(preview.items[0].status, "blocked", "{destination}");
            assert!(preview.items[0].reason.contains("无法完成引用检查"));
        }
    }

    #[test]
    fn filesystem_reference_scan_reports_budget_exhaustion() {
        let (_temp, engine, _, backup) = setup();
        let before = engine.snapshot().unwrap();
        let root = Path::new(&before.storage_root);
        let mut after = before.clone();
        let mut changes = vec![];
        engine
            .prepare_restore(&mut after, root, &mut changes, &backup)
            .unwrap();
        let candidates = before
            .skills
            .iter()
            .map(|s| s.bundle_digest.clone())
            .collect();
        let error = Engine::filesystem_cleanup_references(
            root,
            &before,
            &changes,
            &candidates,
            None,
            &mut BTreeMap::new(),
            0,
        )
        .unwrap_err();
        assert!(error.to_string().contains("扫描上限"));
    }
}

#[cfg(test)]
#[path = "object_cleanup_tests.rs"]
mod execution_tests;
