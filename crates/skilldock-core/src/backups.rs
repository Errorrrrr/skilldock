use super::*;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupEntry {
    id: String,
    created_at: String,
    status: String,
    paths: Vec<String>,
    issues: Vec<String>,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct BackupRemoval {
    path: PathBuf,
    entries: Vec<files::TreeEntry>,
}

impl Engine {
    pub(crate) fn prune_backups(&self, root: &Path, state: &Snapshot) -> Result<()> {
        let journals = self.backup_journals(root)?;
        let mut kept = 0;
        let mut errors = vec![];
        for mut journal in journals {
            if journal.status == "restored"
                || state.tasks.iter().any(|t| {
                    t.kind == "restore_backup" && t.status == "success" && t.message == journal.id
                })
            {
                continue;
            }
            if journal.status == "committed" {
                kept += 1;
                if kept <= state.settings.backup_retention {
                    continue;
                }
            } else if journal.status != "pruning" {
                continue;
            }
            if let Err(error) = self.prune_backup(root, &mut journal) {
                journal.error = error.to_string();
                files::atomic_json(
                    &root
                        .join("transactions")
                        .join(format!("{}.json", journal.id)),
                    &journal,
                )?;
                errors.push(error.to_string());
            }
        }
        if !errors.is_empty() {
            return fail(errors.join("；"));
        }
        Ok(())
    }

    fn prune_backup(&self, root: &Path, journal: &mut Journal) -> Result<()> {
        let log = root
            .join("transactions")
            .join(format!("{}.json", journal.id));
        for c in journal
            .changes
            .iter()
            .filter(|c| c.backup.is_some() && !c.restore)
        {
            let backup = c.backup.as_ref().unwrap();
            if backup.parent() != c.path.parent()
                || !backup
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("")
                    .starts_with(".skilldock-backup-")
                || (files::exists(backup) && fs::canonicalize(backup)? != *backup)
            {
                return fail(format!("备份路径异常，未清理：{}", backup.display()));
            }
        }
        if journal.pruning.is_none() {
            let mut plans = vec![];
            for c in journal
                .changes
                .iter()
                .filter(|c| c.backup.is_some() && !c.restore)
            {
                let backup = c.backup.as_ref().unwrap();
                if !files::exists(backup) {
                    continue;
                }
                let binding = journal
                    .after
                    .bindings
                    .iter()
                    .find(|b| Path::new(&b.path) == c.path)
                    .ok_or_else(|| error::Error::Message("备份缺少原始记录".into()))?;
                let entries = if journal.status == "pruning" {
                    // Recover old interrupted deletions using the preserved library
                    // as the deletion inventory. Unknown extra files stop this batch.
                    files::tree_manifest(&binding_path(root, binding), false)?
                } else {
                    self.validate_backup(root, c, binding, false)?;
                    files::tree_manifest(backup, false)?
                };
                plans.push(BackupRemoval {
                    path: backup.clone(),
                    entries,
                });
            }
            journal.pruning = Some(plans);
            journal.status = "pruning".into();
            files::atomic_json(&log, journal)?;
        }
        let plans = journal.pruning.as_ref().unwrap();
        // Missing entries are already removed; additional or changed entries are
        // external modifications and must survive a retry.
        for plan in plans {
            if !journal
                .changes
                .iter()
                .any(|c| c.backup.as_ref() == Some(&plan.path) && !c.restore)
            {
                return fail("清理清单与备份路径不符");
            }
            if !files::exists(&plan.path) {
                continue;
            }
            for entry in files::tree_manifest(&plan.path, false)? {
                if !plan.entries.contains(&entry) {
                    return fail(format!(
                        "清理期间备份内容变化，已保留：{}",
                        plan.path.display()
                    ));
                }
            }
        }
        for plan in plans {
            if !files::exists(&plan.path) {
                continue;
            }
            for entry in plan.entries.iter().rev() {
                let path = plan.path.join(files::safe_relative(&entry.relative)?);
                if !files::exists(&path) {
                    continue;
                }
                match entry.kind.as_str() {
                    "directory" => fs::remove_dir(&path)?,
                    "link" => files::remove_link(&path)?,
                    "file" => fs::remove_file(&path)?,
                    _ => return fail("不支持的清理条目"),
                }
            }
            fs::remove_dir(&plan.path)?;
        }
        journal.status = "expired".into();
        journal.error.clear();
        files::atomic_json(&log, journal)?;
        Ok(())
    }

    fn backup_journals(&self, root: &Path) -> Result<Vec<Journal>> {
        let mut journals = vec![];
        for entry in fs::read_dir(root.join("transactions"))? {
            let path = entry?.path();
            if path.extension().and_then(|v| v.to_str()) != Some("json") {
                continue;
            }
            let journal: Journal = serde_json::from_slice(&fs::read(path)?)?;
            if journal
                .changes
                .iter()
                .any(|c| c.backup.is_some() && !c.restore)
            {
                journals.push(journal);
            }
        }
        journals.sort_by_key(|j| {
            std::cmp::Reverse(
                j.after
                    .tasks
                    .iter()
                    .find(|t| t.id == j.id)
                    .map(|t| t.created_at.clone())
                    .unwrap_or_default(),
            )
        });
        Ok(journals)
    }
    fn validate_backup(
        &self,
        root: &Path,
        change: &Change,
        binding: &Binding,
        allow_legacy_extra: bool,
    ) -> Result<()> {
        let backup = change
            .backup
            .as_ref()
            .ok_or_else(|| error::Error::Message("缺少备份路径".into()))?;
        if let Some(expected) = &change.backup_digest {
            if files::manifest_digest(backup)? != *expected {
                return fail(format!("备份内容已变化：{}", backup.display()));
            }
            if !allow_legacy_extra
                && files::tree_manifest(backup, false)?
                    != files::tree_manifest(&binding_path(root, binding), false)?
            {
                return fail("统一库副本与备份不一致，保留备份");
            }
        } else {
            // Old imports omitted dependency directories. They may be restored,
            // but must never be pruned while holding content absent from the library.
            let original = binding_path(root, binding);
            if files::tree_manifest(backup, allow_legacy_extra)?
                != files::tree_manifest(&original, allow_legacy_extra)?
            {
                return fail(format!(
                    "备份包含差异或统一库未保存的内容，已保留：{}",
                    backup.display()
                ));
            }
        }
        Ok(())
    }

    fn backup_changes(&self, journal: &Journal, state: &Snapshot) -> Result<Vec<Change>> {
        if journal.status != "committed" {
            return fail("该备份已恢复、已清理或归集未完成");
        }
        if state
            .tasks
            .iter()
            .any(|t| t.kind == "restore_backup" && t.status == "success" && t.message == journal.id)
        {
            return fail("该备份已经恢复");
        }
        let mut changes = vec![];
        for c in journal
            .changes
            .iter()
            .filter(|c| c.backup.is_some() && !c.restore)
        {
            let backup = c.backup.as_ref().unwrap();
            if !fs::symlink_metadata(backup)
                .map(|m| m.is_dir() && !m.file_type().is_symlink())
                .unwrap_or(false)
            {
                return fail(format!("备份缺失或不是实体目录：{}", backup.display()));
            }
            if c.path
                .parent()
                .map(|p| fs::canonicalize(p).ok().as_deref() == Some(p))
                != Some(true)
            {
                return fail(format!("原目录的父路径已变化：{}", c.path.display()));
            }
            let old_binding = journal
                .after
                .bindings
                .iter()
                .find(|b| Path::new(&b.path) == c.path)
                .ok_or_else(|| error::Error::Message("归集记录缺少分发信息".into()))?;
            let binding = state.bindings.iter().find(|b| Path::new(&b.path) == c.path);
            if let Some(binding) = binding {
                if binding.skill_id != old_binding.skill_id {
                    return fail("原位置已被其他 Skill 使用");
                }
                if binding.claims.iter().any(|claim| claim != "manual") {
                    return fail(format!(
                        "原目录被预设使用，请先取消分发：{}",
                        c.path.display()
                    ));
                }
            }
            let expected = if files::exists(&c.path) {
                let binding = binding.ok_or_else(|| {
                    error::Error::Message(format!("原位置存在不受管理的内容：{}", c.path.display()))
                })?;
                let expected = binding_path(Path::new(&state.storage_root), binding);
                if fs::read_link(&c.path).ok().as_ref() != Some(&expected) {
                    return fail(format!("原位置已被修改：{}", c.path.display()));
                }
                Some(expected)
            } else {
                None
            };
            self.validate_backup(Path::new(&state.storage_root), c, old_binding, true)?;
            let backup_digest = files::manifest_digest(backup)?;
            changes.push(Change {
                path: c.path.clone(),
                before: None,
                after: expected,
                backup: Some(backup.clone()),
                restore: true,
                backup_digest: Some(backup_digest),
            });
        }
        Ok(changes)
    }
    pub fn list_backups(&self) -> Result<Vec<BackupEntry>> {
        let Some(root) = self.root()? else {
            return Ok(vec![]);
        };
        let state = self.snapshot()?;
        self.backup_journals(&root)?
            .iter()
            .map(|journal| {
                let restored = journal.status == "restored"
                    || state.tasks.iter().any(|t| {
                        t.kind == "restore_backup"
                            && t.status == "success"
                            && t.message == journal.id
                    });
                let status = if restored {
                    "restored"
                } else if journal.status == "pruning" {
                    "pruning"
                } else if journal.status == "expired" {
                    "expired"
                } else if journal.status == "committed" {
                    "available"
                } else {
                    "unavailable"
                };
                let issues = if status == "available" {
                    self.backup_changes(journal, &state)
                        .err()
                        .map(|e| vec![e.to_string()])
                        .unwrap_or_default()
                } else if !journal.error.is_empty() {
                    vec![journal.error.clone()]
                } else {
                    vec![]
                };
                Ok(BackupEntry {
                    id: journal.id.clone(),
                    created_at: journal
                        .after
                        .tasks
                        .iter()
                        .find(|t| t.id == journal.id)
                        .map(|t| t.created_at.clone())
                        .unwrap_or_default(),
                    status: if status == "available" && !issues.is_empty() {
                        "conflict".into()
                    } else {
                        status.into()
                    },
                    paths: journal
                        .changes
                        .iter()
                        .filter(|c| c.backup.is_some())
                        .map(|c| c.path.display().to_string())
                        .collect(),
                    issues,
                })
            })
            .collect()
    }
    pub fn restore_backup(&self, backup_id: &str) -> Result<Snapshot> {
        self.restore_backup_options(backup_id, None)
    }
    pub fn restore_backup_options(
        &self,
        backup_id: &str,
        revision: Option<u32>,
    ) -> Result<Snapshot> {
        self.restore_backup_cleanup(backup_id, revision, vec![])
    }
    pub fn restore_backup_cleanup(
        &self,
        backup_id: &str,
        revision: Option<u32>,
        choices: Vec<object_cleanup::CleanupChoice>,
    ) -> Result<Snapshot> {
        if !choices.is_empty() && revision.is_none() {
            return fail("永久清理需要有效的预览版本");
        }
        self.transact_with_cleanup(
            "restore_backup",
            "恢复归集备份",
            |state, root, changes, cleanup| {
                if revision.is_some_and(|revision| revision != state.revision) {
                    return fail("状态已变化，请重新预览恢复内容");
                }
                let before = state.clone();
                self.prepare_restore(state, root, changes, backup_id)?;
                if !choices.is_empty() {
                    let plan = self.plan_restore_cleanup(root, &before, state, changes)?;
                    *cleanup =
                        Some(self.prepare_object_cleanup(root, &before, changes, &plan, choices)?);
                }
                Ok(())
            },
        )
    }
    pub(crate) fn prepare_restore(
        &self,
        state: &mut Snapshot,
        root: &Path,
        changes: &mut Vec<Change>,
        backup_id: &str,
    ) -> Result<()> {
        let backup_id = uuid::Uuid::parse_str(backup_id)
            .map_err(|_| error::Error::Message("无效备份 ID".into()))?
            .to_string();
        let journal: Journal = serde_json::from_slice(&fs::read(
            root.join("transactions").join(format!("{backup_id}.json")),
        )?)?;
        *changes = self.backup_changes(&journal, state)?;
        if changes.is_empty() {
            return fail("该批次没有可恢复的原目录");
        }
        state
            .bindings
            .retain(|b| !changes.iter().any(|c| Path::new(&b.path) == c.path));
        let added_ids: std::collections::BTreeSet<_> = journal
            .after
            .skills
            .iter()
            .filter(|skill| !journal.before.skills.iter().any(|old| old.id == skill.id))
            .map(|skill| skill.id.clone())
            .collect();
        state.skills.retain(|skill| {
            !added_ids.contains(&skill.id)
                || state
                    .bindings
                    .iter()
                    .any(|binding| binding.skill_id == skill.id)
                || state.presets.iter().any(|preset| {
                    preset.skill_ids.contains(&skill.id)
                        || preset.locks.values().any(|locked| locked.id == skill.id)
                })
        });
        let added_sources: std::collections::BTreeSet<_> = journal
            .after
            .sources
            .iter()
            .filter(|source| !journal.before.sources.iter().any(|old| old.id == source.id))
            .map(|source| source.id.clone())
            .collect();
        state.sources.retain(|source| {
            if source.local_member_ids.is_some() {
                return true;
            }
            !added_sources.contains(&source.id)
                || state
                    .skills
                    .iter()
                    .any(|skill| skill.source_id == source.id)
                || state.presets.iter().any(|preset| {
                    preset
                        .locks
                        .values()
                        .any(|skill| skill.source_id == source.id)
                })
        });
        for source in &mut state.sources {
            if source.kind != "local" || source.status != "detached" {
                continue;
            }
            let before = journal
                .before
                .sources
                .iter()
                .find(|old| old.id == source.id && old.path == source.path);
            if !journal.after.sources.iter().any(|after| {
                after.id == source.id && after.path == source.path && after.status == "detached"
            }) {
                continue;
            }
            let still_adopted = state.bindings.iter().any(|binding| {
                Path::new(&binding.path).starts_with(Path::new(&source.path))
                    && state
                        .skills
                        .iter()
                        .any(|skill| skill.id == binding.skill_id && skill.source_id == source.id)
            });
            if !still_adopted {
                if source.policy.mode == "off" {
                    if let Some(previous) = before {
                        source.policy = previous.policy.clone();
                    }
                }
                source.status = before
                    .filter(|old| old.status != "detached")
                    .map(|old| old.status.clone())
                    .unwrap_or_else(|| "current".into());
                source.error = before
                    .filter(|old| old.status == "error")
                    .map(|old| old.error.clone())
                    .unwrap_or_default();
                source.next_check.clear();
            }
        }
        // Record the restored batch in the transaction itself, without rewinding later state.
        state.tasks.push(Task {
            id: id(),
            kind: "restore_backup".into(),
            title: "已还原原目录，并移除无引用的归集记录".into(),
            status: "success".into(),
            message: backup_id.clone(),
            created_at: now(),
        });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn setup() -> (tempfile::TempDir, Engine, PathBuf) {
        let temp = tempfile::tempdir().unwrap();
        let engine = Engine::new(Some(temp.path().join("config"))).unwrap();
        engine
            .configure(temp.path().join("library").to_str().unwrap())
            .unwrap();
        engine
            .transact("test", "isolate configured scan roots", |state, _, _| {
                state.settings.agent_profiles.clear();
                Ok(())
            })
            .unwrap();
        let source = fs::canonicalize(temp.path()).unwrap().join("tools");
        fs::create_dir_all(&source).unwrap();
        (temp, engine, source)
    }
    fn adopt(engine: &Engine, source: &Path, name: &str) -> String {
        let dir = source.join(name);
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("SKILL.md"),
            format!("---\nname: {name}\ndescription: Test\n---\n# {name}"),
        )
        .unwrap();
        engine.import_folder(&dir, vec![], true, None).unwrap();
        engine
            .list_backups()
            .unwrap()
            .into_iter()
            .find(|b| b.paths.contains(&dir.display().to_string()))
            .unwrap()
            .id
    }
    #[test]
    fn multiple_sources_form_one_backup_and_restore_together() {
        let (_temp, engine, source) = setup();
        let mut groups = vec![];
        for name in ["agent-a", "agent-b"] {
            let path = source.join(name);
            fs::create_dir_all(&path).unwrap();
            fs::write(path.join("SKILL.md"), format!("# {name}")).unwrap();
            groups.push(json!({"path": path, "selectedPaths": [path]}));
        }
        engine
            .import_batch(&json!({"groups": groups, "adopt": true}))
            .unwrap();
        let backups = engine.list_backups().unwrap();
        assert_eq!(backups.len(), 1);
        assert_eq!(backups[0].paths.len(), 2);
        engine.restore_backup(&backups[0].id).unwrap();
        for name in ["agent-a", "agent-b"] {
            assert!(
                !fs::symlink_metadata(source.join(name))
                    .unwrap()
                    .file_type()
                    .is_symlink()
            );
        }
        assert!(engine.snapshot().unwrap().skills.is_empty());
    }

    #[test]
    fn invalid_source_does_not_partially_adopt_batch() {
        let (_temp, engine, source) = setup();
        let valid = source.join("valid");
        fs::create_dir_all(&valid).unwrap();
        fs::write(valid.join("SKILL.md"), "# Valid").unwrap();
        assert!(
            engine
                .import_batch(&json!({"adopt": true, "groups": [
                    {"path": valid, "selectedPaths": [valid]},
                    {"path": source.join("missing"), "selectedPaths": [source.join("missing")]}
                ]}))
                .is_err()
        );
        assert!(
            !fs::symlink_metadata(valid)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert!(engine.snapshot().unwrap().skills.is_empty());
        assert!(engine.list_backups().unwrap().is_empty());
    }

    #[test]
    fn unreferenced_records_are_removed_but_preset_references_remain() {
        let (_temp, engine, source) = setup();
        let backup = adopt(&engine, &source, "first");
        let skill = engine.snapshot().unwrap().skills[0].clone();
        engine
            .transact("test", "test", |state, _, _| {
                state.presets.push(Preset {
                    id: id(),
                    name: "keep".into(),
                    description: String::new(),
                    skill_ids: vec![skill.id.clone()],
                    revision: 1,
                    locks: Default::default(),
                });
                Ok(())
            })
            .unwrap();
        let state = engine.restore_backup(&backup).unwrap();
        assert_eq!(state.skills.len(), 1);
        assert_eq!(state.sources.len(), 1);
        assert!(state.bindings.is_empty());
        assert!(skill_path(Path::new(&state.storage_root), &skill).is_dir());
    }

    #[test]
    fn another_distribution_keeps_the_library_skill() {
        let (_temp, engine, source) = setup();
        let backup = adopt(&engine, &source, "first");
        engine
            .transact("test", "test", |state, _, _| {
                let mut other = state.bindings[0].clone();
                other.id = id();
                other.path = source.join("other").display().to_string();
                state.bindings.push(other);
                Ok(())
            })
            .unwrap();
        let state = engine.restore_backup(&backup).unwrap();
        assert_eq!(state.skills.len(), 1);
        assert_eq!(state.bindings.len(), 1);
    }

    #[test]
    fn cleanup_candidate_is_scoped_and_shared_bundle_is_preserved() {
        for shared in [false, true] {
            let (_temp, engine, source) = setup();
            let backup = adopt(&engine, &source, "first");
            let imported = engine.snapshot().unwrap().skills[0].clone();
            let root = engine.root().unwrap().unwrap();
            let object = root.join("objects").join(&imported.bundle_digest);
            if shared {
                engine
                    .transact("test", "test", |state, _, _| {
                        let mut alias = imported.clone();
                        alias.id = id();
                        alias.name = "Shared alias".into();
                        state.skills.push(alias);
                        Ok(())
                    })
                    .unwrap();
            }
            let preview = engine.preview_restore(&backup).unwrap();
            let item = preview
                .items
                .iter()
                .find(|item| item.digest == imported.bundle_digest)
                .unwrap();
            assert_eq!(item.status, if shared { "referenced" } else { "ready" });
            engine.restore_backup(&backup).unwrap();
            assert!(object.is_dir()); // Preview never deletes files.
            assert!(source.join("first/SKILL.md").is_file());
        }
    }

    #[test]
    fn dependencies_survive_adoption_and_backup_retention() {
        let (_temp, engine, source) = setup();
        let original = source.join("runtime");
        fs::create_dir_all(original.join("node_modules/package")).unwrap();
        fs::create_dir_all(original.join("target")).unwrap();
        fs::write(original.join("SKILL.md"), "# Runtime").unwrap();
        fs::write(original.join("node_modules/package/index.js"), "dependency").unwrap();
        fs::write(original.join("target/tool"), "binary").unwrap();
        engine.import_folder(&original, vec![], true, None).unwrap();
        for name in ["b", "c", "d"] {
            adopt(&engine, &source, name);
        }
        assert_eq!(
            fs::read_to_string(original.join("node_modules/package/index.js")).unwrap(),
            "dependency"
        );
        assert_eq!(
            fs::read_to_string(original.join("target/tool")).unwrap(),
            "binary"
        );
        assert!(
            engine
                .list_backups()
                .unwrap()
                .iter()
                .any(|b| b.status == "expired")
        );
    }

    #[test]
    fn legacy_unrepresented_dependencies_are_retained_and_restorable() {
        let (_temp, engine, source) = setup();
        let backup = adopt(&engine, &source, "legacy");
        let root = engine.root().unwrap().unwrap();
        let path = root.join("transactions").join(format!("{backup}.json"));
        let mut journal: Journal = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        journal.changes[0].backup_digest = None;
        let folder = journal.changes[0].backup.as_ref().unwrap();
        fs::create_dir_all(folder.join("node_modules/p")).unwrap();
        fs::write(folder.join("node_modules/p/runtime"), "legacy-only").unwrap();
        files::atomic_json(&path, &journal).unwrap();
        for name in ["b", "c", "d"] {
            adopt(&engine, &source, name);
        }
        assert!(folder.join("node_modules/p/runtime").is_file());
        engine.restore_backup(&backup).unwrap();
        assert_eq!(
            fs::read_to_string(source.join("legacy/node_modules/p/runtime")).unwrap(),
            "legacy-only"
        );
    }

    #[test]
    fn sibling_links_restore_after_original_distribution_is_revoked() {
        let (_temp, engine, source) = setup();
        let original = source.join("sibling");
        fs::create_dir_all(&original).unwrap();
        fs::create_dir_all(source.join("shared")).unwrap();
        fs::write(original.join("SKILL.md"), "# Sibling").unwrap();
        fs::write(source.join("shared/helper"), "resource").unwrap();
        files::create_link(
            Path::new("../shared/helper"),
            &original.join("helper"),
            false,
        )
        .unwrap();
        let state = engine
            .import_folder(&source, vec![original.display().to_string()], true, None)
            .unwrap();
        let backup = engine.list_backups().unwrap().remove(0);
        engine
            .execute_local(
                "revoke",
                &json!({"bindingIds": [state.bindings[0].id], "claim": "manual"}),
            )
            .unwrap();
        assert!(!files::exists(&original));
        let changes = engine
            .backup_changes(
                &engine
                    .backup_journals(&engine.root().unwrap().unwrap())
                    .unwrap()[0],
                &engine.snapshot().unwrap(),
            )
            .unwrap();
        files::apply(&changes[0]).unwrap();
        files::undo(&changes[0]).unwrap();
        assert!(!files::exists(&original));
        engine.restore_backup(&backup.id).unwrap();
        assert_eq!(
            fs::read_to_string(original.join("helper")).unwrap(),
            "resource"
        );
    }

    #[test]
    fn partial_pruning_retries_and_another_modified_batch_is_preserved() {
        let (_temp, engine, source) = setup();
        engine
            .execute_local("settings", &json!({"backupRetention": 100}))
            .unwrap();
        let oldest = adopt(&engine, &source, "oldest");
        let changed = adopt(&engine, &source, "changed");
        adopt(&engine, &source, "latest");
        let root = engine.root().unwrap().unwrap();
        let oldest_path = root.join("transactions").join(format!("{oldest}.json"));
        let mut journal: Journal =
            serde_json::from_slice(&fs::read(&oldest_path).unwrap()).unwrap();
        let folder = journal.changes[0].backup.as_ref().unwrap().clone();
        journal.pruning = Some(vec![BackupRemoval {
            path: folder.clone(),
            entries: files::tree_manifest(&folder, false).unwrap(),
        }]);
        journal.status = "pruning".into();
        files::atomic_json(&oldest_path, &journal).unwrap();
        fs::remove_file(folder.join("SKILL.md")).unwrap();
        let changed_log: Journal = serde_json::from_slice(
            &fs::read(root.join("transactions").join(format!("{changed}.json"))).unwrap(),
        )
        .unwrap();
        let changed_folder = changed_log.changes[0].backup.as_ref().unwrap();
        fs::write(changed_folder.join("external"), "keep").unwrap();
        engine
            .execute_local("settings", &json!({"backupRetention": 1}))
            .unwrap();
        assert!(!folder.exists());
        assert!(changed_folder.join("external").is_file());
        assert_eq!(
            engine
                .list_backups()
                .unwrap()
                .iter()
                .find(|b| b.id == oldest)
                .unwrap()
                .status,
            "expired"
        );
    }

    #[test]
    fn legacy_rollback_state_can_be_recovered_and_writes_resume() {
        let (_temp, engine, source) = setup();
        let backup = adopt(&engine, &source, "first");
        let root = engine.root().unwrap().unwrap();
        let path = root.join("transactions").join(format!("{backup}.json"));
        let mut journal: Journal = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        files::undo(&journal.changes[0]).unwrap();
        journal.status = "rolledBack".into();
        files::atomic_json(&path, &journal).unwrap();
        let mut state = journal.before.clone();
        state.tasks.push(Task {
            id: backup.clone(),
            kind: "import".into(),
            title: "interrupted".into(),
            status: "needsRecovery".into(),
            message: String::new(),
            created_at: now(),
        });
        files::atomic_json(&root.join("state.json"), &state).unwrap();
        engine.recover(&backup).unwrap();
        assert!(
            !engine
                .snapshot()
                .unwrap()
                .tasks
                .iter()
                .any(|t| t.status == "needsRecovery")
        );
        engine
            .execute_local("settings", &json!({"backupRetention": 3}))
            .unwrap();
    }

    #[tokio::test]
    async fn restoring_local_source_keeps_automatic_updates_disabled() {
        let (_temp, engine, source) = setup();
        let original = source.join("existing");
        fs::create_dir_all(&original).unwrap();
        fs::write(original.join("SKILL.md"), "# Existing").unwrap();
        let state = engine
            .import_folder(&original, vec![], false, None)
            .unwrap();
        let sid = state.sources[0].id.clone();
        engine
            .execute_local(
                "set_policy",
                &json!({"sourceId": sid, "mode": "off", "intervalHours": 12}),
            )
            .unwrap();
        engine.import_folder(&original, vec![], true, None).unwrap();
        let backup = engine.list_backups().unwrap().remove(0);
        engine.restore_backup(&backup.id).unwrap();
        let state = engine.check_source(&sid, false).await.unwrap();
        assert_eq!(state.sources[0].status, "current");
        assert_eq!(state.sources[0].policy.mode, "off");
        assert_eq!(state.sources[0].policy.interval_hours, 12);
    }

    #[tokio::test]
    async fn preset_retained_new_source_is_available_after_restore() {
        let (_temp, engine, source) = setup();
        let backup = adopt(&engine, &source, "first");
        let skill = engine.snapshot().unwrap().skills[0].clone();
        engine
            .execute_local(
                "save_preset",
                &json!({"name": "keep", "description": "", "skillIds": [skill.id]}),
            )
            .unwrap();
        let state = engine.restore_backup(&backup).unwrap();
        assert_eq!(state.sources[0].status, "current");
        engine.check_source(&skill.source_id, false).await.unwrap();
    }

    #[test]
    fn legacy_backup_larger_than_new_import_limit_can_be_restored() {
        let (_temp, engine, source) = setup();
        let backup = adopt(&engine, &source, "legacy-large");
        let root = engine.root().unwrap().unwrap();
        let path = root.join("transactions").join(format!("{backup}.json"));
        let mut journal: Journal = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        journal.changes[0].backup_digest = None;
        let folder = journal.changes[0].backup.as_ref().unwrap();
        fs::create_dir_all(folder.join("node_modules")).unwrap();
        fs::File::create(folder.join("node_modules/large.bin"))
            .unwrap()
            .set_len(256 * 1024 * 1024 + 1)
            .unwrap();
        files::atomic_json(&path, &journal).unwrap();
        engine.restore_backup(&backup).unwrap();
        assert_eq!(
            fs::metadata(source.join("legacy-large/node_modules/large.bin"))
                .unwrap()
                .len(),
            256 * 1024 * 1024 + 1
        );
    }

    #[test]
    fn restores_original_and_preserves_later_state() {
        let (_temp, engine, source) = setup();
        let backup = adopt(&engine, &source, "first");
        adopt(&engine, &source, "second");
        let state = engine.restore_backup(&backup).unwrap();
        assert_eq!(state.skills.len(), 1);
        assert_eq!(state.bindings.len(), 1);
        assert!(
            !fs::symlink_metadata(source.join("first"))
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert!(fs::read_link(source.join("second")).is_ok());
        assert!(engine.restore_backup(&backup).is_err());
    }
    #[test]
    fn keeps_three_batches_and_respects_configured_count() {
        let (_temp, engine, source) = setup();
        for name in ["a", "b", "c", "d"] {
            adopt(&engine, &source, name);
        }
        let backups = engine.list_backups().unwrap();
        assert_eq!(
            backups.iter().filter(|b| b.status == "available").count(),
            3
        );
        assert_eq!(backups.iter().filter(|b| b.status == "expired").count(), 1);
        engine
            .execute_local("settings", &json!({"backupRetention": 1}))
            .unwrap();
        assert_eq!(
            engine
                .list_backups()
                .unwrap()
                .iter()
                .filter(|b| b.status == "available")
                .count(),
            1
        );
        assert!(
            engine
                .execute_local("settings", &json!({"backupRetention": 0}))
                .is_err()
        );
    }
    #[test]
    fn changed_original_is_not_overwritten() {
        let (_temp, engine, source) = setup();
        let backup = adopt(&engine, &source, "first");
        files::remove_link(&source.join("first")).unwrap();
        fs::create_dir(source.join("first")).unwrap();
        fs::write(source.join("first/external"), "keep").unwrap();
        assert!(engine.restore_backup(&backup).is_err());
        assert_eq!(
            fs::read_to_string(source.join("first/external")).unwrap(),
            "keep"
        );
        assert_eq!(engine.list_backups().unwrap()[0].status, "conflict");
    }
    #[test]
    fn interrupted_restore_is_recovered_through_task_recovery() {
        let (_temp, engine, source) = setup();
        let backup = adopt(&engine, &source, "first");
        let root = engine.root().unwrap().unwrap();
        let original = engine.backup_journals(&root).unwrap().remove(0);
        let before = engine.snapshot().unwrap();
        let changes = engine.backup_changes(&original, &before).unwrap();
        let recovery_id = id();
        let journal = Journal {
            object_cleanup: None,
            pruning: None,
            id: recovery_id.clone(),
            status: "running".into(),
            changes,
            before: before.clone(),
            after: before,
            error: "模拟恢复中断".into(),
        };
        files::atomic_json(
            &root
                .join("transactions")
                .join(format!("{recovery_id}.json")),
            &journal,
        )
        .unwrap();
        files::apply(&journal.changes[0]).unwrap();
        assert!(
            engine
                .snapshot()
                .unwrap()
                .tasks
                .iter()
                .any(|t| t.id == recovery_id && t.status == "needsRecovery")
        );
        engine.recover(&recovery_id).unwrap();
        assert!(fs::read_link(source.join("first")).is_ok());
        engine.restore_backup(&backup).unwrap();
        assert!(fs::read_link(source.join("first")).is_err());
    }
    #[test]
    fn restore_change_can_rollback_after_link_removal_or_full_restore() {
        let (_temp, engine, source) = setup();
        let backup = adopt(&engine, &source, "first");
        let root = engine.root().unwrap().unwrap();
        let journal = engine
            .backup_journals(&root)
            .unwrap()
            .into_iter()
            .find(|j| j.id == backup)
            .unwrap();
        let change = engine
            .backup_changes(&journal, &engine.snapshot().unwrap())
            .unwrap()
            .remove(0);
        files::remove_link(&source.join("first")).unwrap();
        files::undo(&change).unwrap();
        assert!(fs::read_link(source.join("first")).is_ok());
        files::apply(&change).unwrap();
        assert!(fs::read_link(source.join("first")).is_err());
        files::undo(&change).unwrap();
        assert!(fs::read_link(source.join("first")).is_ok());
        assert!(change.backup.unwrap().is_dir());
    }
}

#[cfg(test)]
mod edge_tests {
    use super::*;
    #[test]
    fn nested_restore_after_migration_and_missing_backup_rejection() {
        let temp = tempfile::tempdir().unwrap();
        let base = fs::canonicalize(temp.path()).unwrap();
        let engine = Engine::new(Some(base.join("config"))).unwrap();
        engine
            .configure(base.join("library").to_str().unwrap())
            .unwrap();
        let parent = base.join("agent/parent");
        let child = parent.join("child");
        fs::create_dir_all(&child).unwrap();
        fs::write(parent.join("SKILL.md"), "# Parent").unwrap();
        fs::write(child.join("SKILL.md"), "# Child").unwrap();
        engine.import_folder(&parent, vec![], true, None).unwrap();
        let backup = engine.list_backups().unwrap().remove(0);
        assert_eq!(backup.paths.len(), 1);
        engine
            .migrate_storage(base.join("new-library").to_str().unwrap())
            .unwrap();
        assert_eq!(engine.list_backups().unwrap()[0].status, "available");
        let state = engine.restore_backup(&backup.id).unwrap();
        assert!(state.skills.is_empty());
        assert!(state.bindings.is_empty());
        assert_eq!(
            fs::read_to_string(child.join("SKILL.md")).unwrap(),
            "# Child"
        );
        engine.import_folder(&parent, vec![], true, None).unwrap();
        let root = engine.root().unwrap().unwrap();
        let journal = engine.backup_journals(&root).unwrap().remove(0);
        fs::remove_dir_all(journal.changes[0].backup.as_ref().unwrap()).unwrap();
        assert!(engine.restore_backup(&journal.id).is_err());
        assert!(fs::read_link(parent).is_ok());
    }
}
