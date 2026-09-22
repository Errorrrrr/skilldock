use crate::*;
use std::io::{Read, Write};

#[derive(Serialize, Deserialize)]
struct Migration {
    id: String,
    status: String,
    old_root: PathBuf,
    new_root: PathBuf,
    changes: Vec<Change>,
    #[serde(default)]
    remove_old: bool,
    #[serde(default)]
    nested_layout: bool,
}
#[derive(Serialize, Deserialize)]
struct PresetPackage {
    schema: u32,
    name: String,
    description: String,
    skills: Vec<Skill>,
    sources: Vec<Source>,
}

impl Engine {
    pub(crate) fn decorate_migration(&self, state: &mut Snapshot) -> Result<()> {
        let path = self.config_dir.join("migration.json");
        if !path.exists() {
            return Ok(());
        }
        let migration: Migration = serde_json::from_slice(&fs::read(path)?)?;
        if matches!(migration.status.as_str(), "running" | "cleanup") {
            state.tasks.retain(|task| task.id != migration.id);
            state.tasks.push(Task {
                id: migration.id,
                kind: "migration".into(),
                title: "存储迁移需要恢复".into(),
                status: "needsRecovery".into(),
                message: format!(
                    "{} → {}",
                    migration.old_root.display(),
                    migration.new_root.display()
                ),
                created_at: now(),
            });
        }
        Ok(())
    }
    pub(crate) fn recover_migration(&self, task_id: &str) -> Result<Option<Snapshot>> {
        let path = self.config_dir.join("migration.json");
        if !path.exists() {
            return Ok(None);
        }
        let mut migration: Migration = serde_json::from_slice(&fs::read(&path)?)?;
        if migration.id != task_id || !matches!(migration.status.as_str(), "running" | "cleanup") {
            return Ok(None);
        }
        let _guard = self.lock()?;
        if self.root()?.as_ref() == Some(&migration.new_root) {
            if migration.remove_old {
                return Ok(Some(self.finish_migration_cleanup(&mut migration)?));
            }
            migration.status = "committed".into();
        } else {
            for change in migration.changes.iter().rev() {
                files::undo(change)?;
            }
            migration.status = "rolledBack".into();
            if migration.nested_layout && migration.new_root.exists() {
                let retained = migration
                    .old_root
                    .join(format!(".skilldock-migration-failed-{}", migration.id));
                fs::rename(&migration.new_root, retained)?;
            }
        }
        files::atomic_json(&path, &migration)?;
        Ok(Some(self.snapshot()?))
    }
    fn finish_migration_cleanup(&self, migration: &mut Migration) -> Result<Snapshot> {
        if self.root()?.as_ref() != Some(&migration.new_root) {
            return fail("新库尚未启用，不能清理旧目录");
        }
        let control = self.config_dir.join("migration.json");
        migration.status = "cleanup".into();
        files::atomic_json(&control, migration)?;
        // Cleanup is a separate forward-only phase: links already point to the
        // verified new library. A retry must never roll them back to a partial old tree.
        if migration.old_root.exists() {
            if fs::symlink_metadata(&migration.old_root)?
                .file_type()
                .is_symlink()
            {
                return fail("旧目录已被替换为软链，未执行清理");
            }
            let cleanup = if migration.nested_layout {
                (|| -> std::io::Result<()> {
                    for name in [
                        "objects",
                        "transactions",
                        "backups",
                        "cache",
                        "trash",
                        "state.json",
                        "manager.lock",
                    ] {
                        let old = migration.old_root.join(name);
                        match fs::symlink_metadata(&old) {
                            Ok(meta) if meta.file_type().is_symlink() => {
                                return Err(std::io::Error::other("旧库管理目录被替换为软链"));
                            }
                            Ok(meta) if meta.is_dir() => fs::remove_dir_all(old)?,
                            Ok(_) => fs::remove_file(old)?,
                            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
                            Err(e) => return Err(e),
                        }
                    }
                    let _ = fs::remove_dir(migration.old_root.join("skills"));
                    Ok(())
                })()
            } else {
                fs::remove_dir_all(&migration.old_root)
            };
            cleanup.map_err(|error| {
                error::Error::Message(format!(
                    "新库已启用，但旧目录清理失败：{error}。请从任务记录重试恢复"
                ))
            })?;
        }
        if !migration.nested_layout
            && migration
                .old_root
                .file_name()
                .is_some_and(|n| n == ".skilldock")
        {
            // Only an empty former public directory is removed; foreign files remain.
            if let Some(parent) = migration.old_root.parent() {
                let _ = fs::remove_dir(parent);
            }
        }
        let mut state: Snapshot =
            serde_json::from_slice(&fs::read(migration.new_root.join("state.json"))?)?;
        if let Some(task) = state.tasks.iter_mut().find(|task| task.id == migration.id) {
            task.status = "success".into();
            task.message = "新库已启用，旧目录已清理".into();
        }
        files::atomic_json(&migration.new_root.join("state.json"), &state)?;
        migration.status = "committed".into();
        files::atomic_json(&control, migration)?;
        Ok(state)
    }
    pub fn migrate_storage(&self, path: &str) -> Result<Snapshot> {
        let _guard = self.lock()?;
        let old_root = self
            .root()?
            .ok_or_else(|| error::Error::Message("尚未初始化，请先设置目录".into()))?;
        if fs::canonicalize(&self.config_dir)?.starts_with(&old_root) {
            return fail("应用配置目录位于旧库内，无法安全清理旧库");
        }
        let library_lock = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(old_root.join("manager.lock"))?;
        fs4::FileExt::try_lock(&library_lock)
            .map_err(|_| error::Error::Message("中央库正在使用".into()))?;
        let mut state = self.snapshot()?;
        if state.tasks.iter().any(|t| t.status == "needsRecovery") {
            return fail("请先恢复未完成任务");
        }
        let destination = files::absolute(path)?;
        Self::check_root(&destination)?;
        let nested_layout = state.schema_version >= 3
            && destination == old_root
            && old_root.file_name().is_some_and(|n| n != ".skilldock");
        let new_root = if state.schema_version >= 3 {
            destination.join(".skilldock")
        } else {
            destination
        };
        Self::check_root(&new_root)?;
        self.check_external_migration(&old_root, &state, &new_root)?;
        if !nested_layout && (new_root.starts_with(&old_root) || old_root.starts_with(&new_root)) {
            return fail("新旧目录不能互相包含");
        }
        if new_root.exists() && fs::read_dir(&new_root)?.next().is_some() {
            return fail("迁移目标必须为空");
        }
        fs::create_dir_all(&new_root)?;
        let new_root = fs::canonicalize(new_root)?;
        self.check_external_migration(&old_root, &state, &new_root)?;
        if !nested_layout && (new_root.starts_with(&old_root) || old_root.starts_with(&new_root)) {
            return fail("新旧目录解析后互相包含");
        }
        for target in &state.targets {
            let p = Path::new(&target.path);
            if p.starts_with(&new_root) || new_root.starts_with(p) {
                return fail("新统一目录与已有分发目标互相包含");
            }
        }
        let mut changes = state
            .bindings
            .iter()
            .filter(|b| b.external_path.is_none())
            .map(|b| Change {
                restore: false,
                backup_digest: None,
                path: PathBuf::from(&b.path),
                before: Some(binding_path(&old_root, b)),
                after: Some(binding_path(&new_root, b)),
                backup: None,
            })
            .collect::<Vec<_>>();
        for (sid, name) in &state.library_entries {
            let skill = state
                .skills
                .iter()
                .find(|s| &s.id == sid)
                .ok_or_else(|| error::Error::Message("库入口归属丢失".into()))?;
            let old = single_content::library_directory(&old_root).join(name);
            let new = single_content::library_directory(&new_root).join(name);
            if files::exists(&old) {
                changes.push(Change {
                    path: old,
                    before: Some(skill_path(&old_root, skill)),
                    after: None,
                    backup: None,
                    restore: false,
                    backup_digest: None,
                });
            }
            changes.push(Change {
                path: new,
                before: None,
                after: Some(skill_path(&new_root, skill)),
                backup: None,
                restore: false,
                backup_digest: None,
            });
        }
        for c in &changes {
            if fs::read_link(&c.path).ok() != c.before
                || (c.before.is_none() && files::exists(&c.path))
            {
                return fail("有异常链接，请先诊断");
            }
        }
        let mut migration = Migration {
            id: id(),
            status: "running".into(),
            old_root: old_root.clone(),
            new_root: new_root.clone(),
            changes,
            remove_old: true,
            nested_layout,
        };
        let control = self.config_dir.join("migration.json");
        files::atomic_json(&control, &migration)?;
        let result = (|| -> Result<()> {
            if nested_layout {
                for name in ["objects", "transactions", "backups", "cache", "trash"] {
                    let source = old_root.join(name);
                    if source.exists() {
                        files::copy_tree(&source, &new_root.join(name))?;
                    }
                }
            } else {
                files::copy_tree(&old_root, &new_root)?;
            }
            for e in fs::read_dir(new_root.join("objects"))? {
                let e = e?;
                if !e.path().join("tree").is_dir() {
                    continue;
                }
                if !files::snapshot_matches(
                    &e.path().join("tree"),
                    &e.file_name().to_string_lossy(),
                )? {
                    return fail("迁移内容校验失败");
                }
            }
            for c in &migration.changes {
                files::apply(c)?;
            }
            state.storage_root = new_root.display().to_string();
            state.revision += 1;
            state.tasks.push(Task {
                id: migration.id.clone(),
                kind: "migration".into(),
                title: "统一目录迁移完成".into(),
                status: "success".into(),
                message: "新库已启用，正在清理旧目录".into(),
                created_at: now(),
            });
            files::atomic_json(&new_root.join("state.json"), &state)?;
            files::atomic_json(
                &self.config_dir.join("config.json"),
                &Config {
                    storage_root: state.storage_root.clone(),
                },
            )?;
            Ok(())
        })();
        if let Err(e) = result {
            let mut undo_errors = vec![];
            for c in migration.changes.iter().rev() {
                if let Err(e) = files::undo(c) {
                    undo_errors.push(e.to_string());
                }
            }
            if undo_errors.is_empty() {
                migration.status = "rolledBack".into();
                files::atomic_json(&control, &migration)?;
                if migration.nested_layout && migration.new_root.exists() {
                    let retained = migration
                        .old_root
                        .join(format!(".skilldock-migration-failed-{}", migration.id));
                    fs::rename(&migration.new_root, retained)?;
                }
            }
            return fail(format!(
                "迁移未完成：{e}。旧库和新目录内容均保留。{}",
                undo_errors.join("；")
            ));
        }
        self.finish_migration_cleanup(&mut migration)
    }
    pub fn export_preset(&self, preset_id: &str, path: &str) -> Result<Value> {
        let _guard = self.lock()?;
        let state = self.snapshot()?;
        let preset = state
            .presets
            .iter()
            .find(|p| p.id == preset_id)
            .ok_or_else(|| error::Error::Message("预设不存在".into()))?;
        let dest = files::absolute(path)?;
        if files::exists(&dest) {
            return fail("导出文件已存在，请选择其他名称");
        }
        let skills = preset
            .skill_ids
            .iter()
            .map(|sid| {
                preset
                    .locks
                    .get(sid)
                    .or_else(|| state.skills.iter().find(|s| &s.id == sid))
                    .cloned()
                    .ok_or_else(|| error::Error::Message("预设成员缺失".into()))
            })
            .collect::<Result<Vec<_>>>()?;
        if skills.iter().any(|skill| skill.external_path.is_some()) {
            return fail("预设含跟随本地内容的引用，不能导出为锁定版本包；请先复制为快照预设");
        }
        let mut sources = state
            .sources
            .iter()
            .filter(|s| skills.iter().any(|k| k.source_id == s.id))
            .cloned()
            .collect::<Vec<_>>();
        for source in &mut sources {
            source.path.clear();
            source.policy = Policy::default();
            source.last_checked.clear();
            source.next_check.clear();
            source.error.clear();
            source.status = "detached".into();
            if !source.url.starts_with("https://") {
                source.url.clear();
            }
        }
        let package = PresetPackage {
            schema: 1,
            name: preset.name.clone(),
            description: preset.description.clone(),
            skills: skills.clone(),
            sources,
        };
        let parent = dest
            .parent()
            .ok_or_else(|| error::Error::Message("导出路径无效".into()))?;
        let mut temp = tempfile::NamedTempFile::new_in(parent)?;
        {
            let mut zip = zip::ZipWriter::new(temp.as_file_mut());
            let options = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated);
            zip.start_file("preset.json", options)
                .map_err(|e| error::Error::Message(e.to_string()))?;
            zip.write_all(&serde_json::to_vec_pretty(&package)?)?;
            for digest in skills
                .iter()
                .map(|s| s.bundle_digest.clone())
                .collect::<std::collections::BTreeSet<_>>()
            {
                let tree = Path::new(&state.storage_root)
                    .join("objects")
                    .join(&digest)
                    .join("tree");
                if !files::snapshot_matches(&tree, &digest)? {
                    return fail("快照发生变化，拒绝导出");
                }
                for entry in walkdir::WalkDir::new(&tree).follow_links(false) {
                    let e = entry.map_err(|e| error::Error::Message(e.to_string()))?;
                    let rel = e.path().strip_prefix(&tree).unwrap();
                    if rel.as_os_str().is_empty() {
                        continue;
                    }
                    let name = format!(
                        "objects/{digest}/tree/{}",
                        rel.to_string_lossy().replace('\\', "/")
                    );
                    if e.file_type().is_symlink() {
                        return fail(
                            "此预设包含资源软链，当前便携包导出暂不支持，请使用来源清单重装",
                        );
                    }
                    if e.file_type().is_dir() {
                        zip.add_directory(format!("{name}/"), options)
                            .map_err(|e| error::Error::Message(e.to_string()))?;
                    } else {
                        let mut opts = options;
                        #[cfg(unix)]
                        {
                            use std::os::unix::fs::PermissionsExt;
                            opts =
                                opts.unix_permissions(fs::metadata(e.path())?.permissions().mode());
                        }
                        zip.start_file(name, opts)
                            .map_err(|e| error::Error::Message(e.to_string()))?;
                        std::io::copy(&mut fs::File::open(e.path())?, &mut zip)?;
                    }
                }
            }
            zip.finish()
                .map_err(|e| error::Error::Message(e.to_string()))?;
        }
        temp.as_file().sync_all()?;
        temp.persist_noclobber(&dest).map_err(|e| e.error)?;
        Ok(json!({"path":dest.display().to_string()}))
    }
    pub fn import_preset(&self, path: &str) -> Result<Snapshot> {
        let path = files::absolute(path)?;
        if fs::metadata(&path)?.len() > 100 * 1024 * 1024 {
            return fail("预设包超过 100 MiB");
        }
        let mut zip = zip::ZipArchive::new(fs::File::open(path)?)
            .map_err(|e| error::Error::Message(e.to_string()))?;
        if zip.len() > 25000 {
            return fail("预设包文件过多");
        }
        let staging = tempfile::tempdir()?;
        let mut size = 0u64;
        let mut seen = std::collections::BTreeSet::new();
        for i in 0..zip.len() {
            let mut file = zip
                .by_index(i)
                .map_err(|e| error::Error::Message(e.to_string()))?;
            size = size.saturating_add(file.size());
            if size > 256 * 1024 * 1024 {
                return fail("预设展开内容超过限制");
            }
            let relative = file
                .enclosed_name()
                .ok_or_else(|| error::Error::Message("预设包路径越界".into()))?;
            let clean = relative.to_string_lossy().to_string();
            files::safe_relative(&clean)?;
            if !seen.insert(clean.to_lowercase()) {
                return fail("预设包存在重复路径");
            }
            if file.unix_mode().is_some_and(|m| m & 0o170000 == 0o120000) {
                return fail("预设包不允许嵌入资源软链");
            }
            let dest = staging.path().join(relative);
            if file.is_dir() {
                fs::create_dir_all(dest)?;
            } else {
                fs::create_dir_all(dest.parent().unwrap())?;
                let mut out = fs::File::create(&dest)?;
                std::io::copy(&mut file.by_ref().take(256 * 1024 * 1024 + 1), &mut out)?;
                #[cfg(unix)]
                if let Some(mode) = file.unix_mode() {
                    use std::os::unix::fs::PermissionsExt;
                    fs::set_permissions(&dest, fs::Permissions::from_mode(mode & 0o777))?;
                }
            }
        }
        let package: PresetPackage =
            serde_json::from_slice(&fs::read(staging.path().join("preset.json"))?)?;
        if package
            .skills
            .iter()
            .any(|skill| skill.external_path.is_some())
        {
            return fail("便携预设不能携带本机外部路径");
        }
        if package.schema != 1 {
            return fail("预设包版本不兼容");
        }
        self.transact("import_preset", "导入预设包", |s, root, _| {
            let same_members = |p: &Preset| {
                p.skill_ids.len() == package.skills.len()
                    && package.skills.iter().all(|incoming| {
                        p.locks.values().any(|existing| {
                            existing.bundle_digest == incoming.bundle_digest
                                && existing.relative_path == incoming.relative_path
                                && existing.name == incoming.name
                        })
                    })
            };
            if s.presets.iter().any(|p| {
                p.name == package.name && p.description == package.description && same_members(p)
            }) {
                return Ok(());
            }
            let mut ids = vec![];
            let mut locks = BTreeMap::new();
            let mut source_map = BTreeMap::new();
            for source in &package.sources {
                let mut source = source.clone();
                let old = source.id.clone();
                source.id = id();
                source.path.clear();
                source.policy = Policy::default();
                source.status = "detached".into();
                source_map.insert(old, source.id.clone());
                s.sources.push(source);
            }
            for skill in &package.skills {
                if skill.external_path.is_some() {
                    return fail("便携预设不能携带本机外部路径");
                }
                files::safe_relative(&skill.relative_path)?;
                files::clean_name(&skill.name)?;
                if skill.bundle_digest.len() != 64
                    || !skill.bundle_digest.chars().all(|c| c.is_ascii_hexdigit())
                {
                    return fail("快照摘要无效");
                }
                let tree = staging
                    .path()
                    .join("objects")
                    .join(&skill.bundle_digest)
                    .join("tree");
                if !files::snapshot_matches(&tree, &skill.bundle_digest)? {
                    return fail("预设包摘要校验失败");
                }
                if !tree.join(&skill.relative_path).join("SKILL.md").is_file() {
                    return fail("预设包成员内容缺失");
                }
                let imported_digest = if files::tree_digest(&tree)? == skill.bundle_digest {
                    files::snapshot_tree(root, &tree)?
                } else {
                    files::snapshot_current_content(root, &tree)?
                };
                if imported_digest != skill.bundle_digest {
                    return fail("预设包内容在导入期间发生变化，请重试");
                }
                let mut skill = skill.clone();
                skill.id = id();
                skill.source_id = source_map
                    .get(&skill.source_id)
                    .cloned()
                    .ok_or_else(|| error::Error::Message("预设来源记录缺失".into()))?;
                ids.push(skill.id.clone());
                locks.insert(skill.id.clone(), skill.clone());
                s.skills.push(skill);
            }
            s.presets.push(Preset {
                id: id(),
                name: package.name,
                description: package.description,
                skill_ids: ids,
                revision: 1,
                locks,
            });
            Ok(())
        })
    }
}
use std::collections::BTreeMap;

#[cfg(test)]
mod migration_cleanup_tests {
    use super::*;

    #[test]
    fn successful_migration_copies_skill_and_removes_old_library() {
        let temp = tempfile::tempdir().unwrap();
        let engine = Engine::new(Some(temp.path().join("config"))).unwrap();
        let old = temp.path().join("old");
        let new = temp.path().join("new");
        engine.configure(old.to_str().unwrap()).unwrap();
        let root = engine.root().unwrap().unwrap();
        let tree = root.join("objects/staging/tree");
        fs::create_dir_all(&tree).unwrap();
        fs::write(tree.join("SKILL.md"), "# Migration example").unwrap();
        let digest = files::tree_digest(&tree).unwrap();
        fs::rename(
            root.join("objects/staging"),
            root.join("objects").join(&digest),
        )
        .unwrap();
        let state = engine.migrate_storage(new.to_str().unwrap()).unwrap();
        assert!(!old.exists());
        assert_eq!(
            fs::read_to_string(
                new.join(".skilldock/objects")
                    .join(digest)
                    .join("tree/SKILL.md")
            )
            .unwrap(),
            "# Migration example"
        );
        assert_eq!(
            state.storage_root,
            fs::canonicalize(new.join(".skilldock"))
                .unwrap()
                .display()
                .to_string()
        );
        assert!(
            state
                .tasks
                .iter()
                .any(|task| task.message == "新库已启用，旧目录已清理")
        );
    }

    #[test]
    fn failed_content_validation_preserves_old_library() {
        let temp = tempfile::tempdir().unwrap();
        let engine = Engine::new(Some(temp.path().join("config"))).unwrap();
        let old = temp.path().join("old");
        engine.configure(old.to_str().unwrap()).unwrap();
        let root = engine.root().unwrap().unwrap();
        let tree = root.join("objects/incorrect-digest/tree");
        fs::create_dir_all(&tree).unwrap();
        fs::write(tree.join("SKILL.md"), "# Keep this skill").unwrap();
        assert!(
            engine
                .migrate_storage(temp.path().join("new").to_str().unwrap())
                .is_err()
        );
        assert!(tree.join("SKILL.md").is_file());
        assert_eq!(
            engine.root().unwrap().unwrap(),
            fs::canonicalize(old.join(".skilldock")).unwrap()
        );
    }

    #[cfg(unix)]
    #[test]
    fn cleanup_failure_keeps_new_library_active_and_can_be_retried() {
        let temp = tempfile::tempdir().unwrap();
        let engine = Engine::new(Some(temp.path().join("config"))).unwrap();
        let new = temp.path().join("new");
        engine.configure(new.to_str().unwrap()).unwrap();
        let protected = temp.path().join("external");
        fs::create_dir_all(&protected).unwrap();
        fs::write(protected.join("keep"), "external data").unwrap();
        let old = temp.path().join("old");
        std::os::unix::fs::symlink(&protected, &old).unwrap();
        let mut migration = Migration {
            nested_layout: false,
            id: "cleanup-retry".into(),
            status: "running".into(),
            old_root: old.clone(),
            new_root: fs::canonicalize(new.join(".skilldock")).unwrap(),
            changes: vec![],
            remove_old: true,
        };
        assert!(engine.finish_migration_cleanup(&mut migration).is_err());
        assert_eq!(
            engine.root().unwrap().unwrap(),
            fs::canonicalize(new.join(".skilldock")).unwrap()
        );
        assert!(
            engine
                .snapshot()
                .unwrap()
                .tasks
                .iter()
                .any(|task| task.id == migration.id && task.status == "needsRecovery")
        );
        fs::remove_file(&old).unwrap();
        fs::create_dir(&old).unwrap();
        engine.recover_migration(&migration.id).unwrap().unwrap();
        assert!(!old.exists());
        assert!(protected.join("keep").is_file());
        assert!(
            !engine
                .snapshot()
                .unwrap()
                .tasks
                .iter()
                .any(|task| task.status == "needsRecovery")
        );
    }
}
