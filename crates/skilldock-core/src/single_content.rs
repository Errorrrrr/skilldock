use super::*;
use std::collections::{BTreeMap, BTreeSet};

fn same_content(a: &Skill, b: &Skill) -> bool {
    match (&a.external_path, &b.external_path) {
        (Some(a), Some(b)) => fs::canonicalize(a)
            .ok()
            .is_some_and(|p| Some(p) == fs::canonicalize(b).ok()),
        (None, None) => {
            !a.bundle_digest.is_empty()
                && a.bundle_digest == b.bundle_digest
                && a.relative_path == b.relative_path
        }
        _ => false,
    }
}

fn import_name(skill: &Skill) -> &str {
    let suffix = format!("--{}", skill.id.chars().take(8).collect::<String>());
    skill.name.strip_suffix(&suffix).unwrap_or(&skill.name)
}

pub(crate) fn resolve_import_ids(selected: &[Skill], state: &Snapshot) -> Vec<String> {
    let mut ids = BTreeSet::new();
    for old in selected {
        if let Some(skill) = state.skills.iter().find(|s| s.id == old.id).or_else(|| {
            state.skills.iter().find(|s| {
                same_content(old, s) && import_name(s).eq_ignore_ascii_case(import_name(old))
            })
        }) {
            ids.insert(skill.id.clone());
        }
    }
    ids.into_iter().collect()
}

fn remap_ids(ids: &mut Vec<String>, mapping: &BTreeMap<String, String>) {
    let mut seen = BTreeSet::new();
    *ids = ids
        .iter()
        .map(|id| mapping.get(id).unwrap_or(id).clone())
        .filter(|id| seen.insert(id.clone()))
        .collect();
}

fn merge_records(state: &mut Snapshot, mapping: &BTreeMap<String, String>) {
    for (old, new) in mapping {
        if old == new {
            continue;
        }
        if let Some(skill) = state.skills.iter().find(|s| &s.id == old) {
            let source_id = skill.source_id.clone();
            let mut origins = state.skill_origins.remove(old).unwrap_or_default();
            origins.push(source_id);
            let list = state.skill_origins.entry(new.clone()).or_default();
            list.extend(origins);
            list.sort();
            list.dedup();
        }
    }
    for binding in &mut state.bindings {
        if let Some(id) = mapping.get(&binding.skill_id) {
            binding.skill_id = id.clone();
        }
    }
    for preset in &mut state.presets {
        remap_ids(&mut preset.skill_ids, mapping);
        preset.locks.clear();
    }
    for package in &mut state.packages {
        for scope in &mut package.scopes {
            // A merged non-primary source must not resurrect its retired record on refresh.
            for skill in &state.skills {
                if skill.source_id == scope.source_id
                    && mapping.get(&skill.id).is_some_and(|id| id != &skill.id)
                {
                    let primary = mapping
                        .get(&skill.id)
                        .and_then(|id| state.skills.iter().find(|s| &s.id == id));
                    let same_member = primary.is_some_and(|s| {
                        s.source_id == skill.source_id && s.relative_path == skill.relative_path
                    });
                    if !same_member && !scope.excluded.contains(&skill.relative_path) {
                        scope.excluded.push(skill.relative_path.clone());
                    }
                }
            }
        }
        remap_ids(&mut package.member_ids, mapping);
        remap_ids(&mut package.missing_member_ids, mapping);
    }
    for subscription in &mut state.preset_packages {
        remap_ids(&mut subscription.selected_ids, mapping);
        remap_ids(&mut subscription.excluded_ids, mapping);
    }
    state
        .skills
        .retain(|s| !mapping.get(&s.id).is_some_and(|id| id != &s.id));
}

pub(crate) fn finalize(
    before: &Snapshot,
    after: &mut Snapshot,
    root: &Path,
    changes: &mut Vec<Change>,
    kind: &str,
) -> Result<()> {
    if after.schema_version < 3 {
        return Ok(());
    }
    // Existing identity wins when an import collects the exact same complete package member.
    let mut ordered = after.skills.clone();
    ordered.sort_by_key(|s| !before.skills.iter().any(|old| old.id == s.id));
    let mut kept: Vec<Skill> = vec![];
    let mut mapping = BTreeMap::new();
    for skill in ordered {
        if let Some(existing) = kept.iter().find(|old| {
            (before.schema_version < 3 || !before.skills.iter().any(|s| s.id == skill.id))
                && import_name(old).eq_ignore_ascii_case(import_name(&skill))
                && same_content(old, &skill)
        }) {
            mapping.insert(skill.id.clone(), existing.id.clone());
        } else {
            kept.push(skill);
        }
    }
    merge_records(after, &mapping);
    let mut names = BTreeSet::new();
    for skill in &mut after.skills {
        if let Some(old) = before.skills.iter().find(|old| {
            old.id == skill.id
                && old
                    .name
                    .ends_with(&format!("--{}", old.id.chars().take(8).collect::<String>()))
        }) {
            skill.name = old.name.clone();
        }
        if !names.insert(skill.name.to_lowercase()) {
            // Different contents remain independent; names never imply content equivalence.
            let mut base = files::clean_name(&skill.name)?;
            while base.len() > 118 {
                base.pop();
            }
            skill.name = format!("{base}--{}", skill.id.chars().take(8).collect::<String>());
            if !names.insert(skill.name.to_lowercase()) {
                return fail("Skill 名称冲突，请修改名称后重试");
            }
        }
        let origins = after.skill_origins.entry(skill.id.clone()).or_default();
        if !origins.contains(&skill.source_id) {
            origins.push(skill.source_id.clone());
        }
    }
    if before.schema_version >= 3
        && !matches!(kind, "undo_content_update" | "replace_current_content")
    {
        let affected: BTreeSet<_> = before
            .skills
            .iter()
            .filter(|old| {
                after
                    .skills
                    .iter()
                    .any(|new| new.id == old.id && !same_content(old, new))
            })
            .map(|s| s.source_id.clone())
            .collect();
        for source_id in affected {
            let skills: Vec<_> = before
                .skills
                .iter()
                .filter(|s| s.source_id == source_id && s.external_path.is_none())
                .cloned()
                .collect();
            let mut checked = BTreeSet::new();
            if skills.is_empty()
                || !skills.iter().all(|s| {
                    !checked.insert(s.bundle_digest.clone())
                        || files::snapshot_matches(
                            &root.join("objects").join(&s.bundle_digest).join("tree"),
                            &s.bundle_digest,
                        )
                        .unwrap_or(false)
                })
            {
                continue;
            }
            after.content_backups.retain(|b| b.source_id != source_id);
            after.content_backups.push(ContentBackup {
                added_skill_ids: after
                    .skills
                    .iter()
                    .filter(|s| {
                        s.source_id == source_id && !before.skills.iter().any(|old| old.id == s.id)
                    })
                    .map(|s| s.id.clone())
                    .collect(),
                source_id,
                created_at: now(),
                skills,
            });
        }
    }
    after
        .content_backups
        .retain(|b| after.skills.iter().any(|s| s.source_id == b.source_id));
    let mut verified = BTreeSet::new();
    for binding in &mut after.bindings {
        let skill = after
            .skills
            .iter()
            .find(|s| s.id == binding.skill_id)
            .ok_or_else(|| error::Error::Message("分发记录引用不存在的 Skill".into()))?;
        let desired = skill_path(root, skill);
        let prior = before.bindings.iter().find(|b| b.id == binding.id);
        let previous = prior
            .map(|b| binding_path(root, b))
            .unwrap_or_else(|| binding_path(root, binding));
        if desired != previous {
            if after
                .packages
                .iter()
                .any(|p| p.member_ids.contains(&skill.id) && !p.issues.is_empty())
            {
                return fail(format!(
                    "「{}」依赖尚未就绪，当前内容和全部分发保持不变",
                    skill.name
                ));
            }
            if skill.external_path.is_none()
                && verified.insert(skill.bundle_digest.clone())
                && !files::snapshot_matches(
                    &root.join("objects").join(&skill.bundle_digest).join("tree"),
                    &skill.bundle_digest,
                )?
            {
                return fail("待使用内容校验失败，未切换分发");
            }
            if let Some(change) = changes
                .iter_mut()
                .rev()
                .find(|c| c.path == Path::new(&binding.path))
            {
                if change.after.is_none() {
                    return fail("同一目标同时撤销与更新，请重试");
                }
                change.after = Some(desired.clone());
            } else {
                let actual = fs::read_link(&binding.path)?;
                let resolved = if actual.is_absolute() {
                    actual.clone()
                } else {
                    Path::new(&binding.path).parent().unwrap().join(&actual)
                };
                if fs::canonicalize(&resolved).ok() != fs::canonicalize(&previous).ok()
                    || !previous.exists()
                {
                    return fail(format!("分发链接已变化，未更新：{}", binding.path));
                }
                changes.push(Change {
                    path: PathBuf::from(&binding.path),
                    before: Some(actual),
                    after: Some(desired),
                    backup: None,
                    restore: false,
                    backup_digest: None,
                });
            }
            if binding.borrowed {
                binding.original_link = Some(previous.display().to_string());
                binding.borrowed = false;
            }
        }
        binding.digest = skill.bundle_digest.clone();
        binding.relative_path = skill.relative_path.clone();
        binding.external_path = skill.external_path.clone();
        binding.version = skill.version.clone();
        binding.follow = true;
    }
    for preset in &mut after.presets {
        preset.locks.clear();
    }
    for application in &mut after.preset_applications {
        application.follow = true;
    }
    Ok(())
}

impl Engine {
    pub(crate) fn preview_single_content(&self) -> Result<Value> {
        let state = self.snapshot()?;
        let mut groups: BTreeMap<String, Vec<Skill>> = BTreeMap::new();
        for skill in &state.skills {
            groups
                .entry(skill.name.to_lowercase())
                .or_default()
                .push(skill.clone());
        }
        let groups: Vec<_> = groups
            .into_values()
            .map(|skills| {
                let identical = skills.iter().all(|s| same_content(&skills[0], s));
                json!({ "name": skills[0].name, "skills": skills, "identical": identical })
            })
            .collect();
        Ok(
            json!({ "revision":state.revision,"alreadyEnabled":state.schema_version >= 3,"groups":groups,
            "bindings":state.bindings,"presets":state.presets,"backupCount":state.content_backups.len() }),
        )
    }

    pub(crate) fn enable_single_content(&self, request: &Value) -> Result<Value> {
        let state = self.transact(
            "enable_single_content",
            "切换为单一当前内容",
            |state, _, _| {
                if request["expectedRevision"].as_u64() != Some(state.revision as u64) {
                    return fail("资料库已变化，请重新预览");
                }
                if state.schema_version >= 3 {
                    return fail("当前库已经使用单一内容模式");
                }
                let choices = request
                    .get("choices")
                    .and_then(Value::as_array)
                    .ok_or_else(|| error::Error::Message("缺少内容选择".into()))?;
                let mut groups: BTreeMap<String, Vec<Skill>> = BTreeMap::new();
                for skill in &state.skills {
                    groups
                        .entry(skill.name.to_lowercase())
                        .or_default()
                        .push(skill.clone());
                }
                let mut mapping = BTreeMap::new();
                for skills in groups.into_values() {
                    if skills.len() < 2 {
                        continue;
                    }
                    let identical = skills.iter().all(|s| same_content(&skills[0], s));
                    let choice = choices.iter().find(|c| {
                        c["name"]
                            .as_str()
                            .is_some_and(|n| n.eq_ignore_ascii_case(&skills[0].name))
                    });
                    if identical {
                        for skill in &skills {
                            mapping.insert(skill.id.clone(), skills[0].id.clone());
                        }
                    } else if let Some(choice) = choice {
                        if choice["keepSeparate"].as_bool() == Some(true) {
                            continue;
                        }
                        let selected = choice["skillId"]
                            .as_str()
                            .filter(|id| skills.iter().any(|s| s.id == *id))
                            .ok_or_else(|| {
                                error::Error::Message(
                                    "请选择保留的当前内容，或分别保留为独立 Skill".into(),
                                )
                            })?;
                        for skill in &skills {
                            mapping.insert(skill.id.clone(), selected.to_string());
                        }
                    } else {
                        return fail(format!("「{}」有不同内容，请明确选择", skills[0].name));
                    }
                }
                merge_records(state, &mapping);
                state.schema_version = 3;
                Ok(())
            },
        )?;
        Ok(serde_json::to_value(state)?)
    }

    pub(crate) fn replace_current_content(&self, request: &Value) -> Result<Value> {
        let state = self.transact(
            "replace_current_content",
            "替换 Skill 当前内容",
            |state, root, _| {
                if state.schema_version < 3 {
                    return fail("请先切换为单一内容模式");
                }
                if request["expectedRevision"].as_u64() != Some(state.revision as u64) {
                    return fail("资料库已变化，请重新确认");
                }
                let sid = text(request, "skillId")?;
                let replacement_id = text(request, "replacementId")?;
                if sid == replacement_id {
                    return fail("请选择另一份已导入内容");
                }
                let old = state
                    .skills
                    .iter()
                    .find(|s| s.id == sid)
                    .cloned()
                    .ok_or_else(|| error::Error::Message("Skill 不存在".into()))?;
                let replacement = state
                    .skills
                    .iter()
                    .find(|s| s.id == replacement_id)
                    .cloned()
                    .ok_or_else(|| error::Error::Message("替换内容不存在".into()))?;
                if old.external_path.is_some() || replacement.external_path.is_some() {
                    return fail("本地引用请先复制入库，不能替换外部目录");
                }
                for skill in [&old, &replacement] {
                    if !files::snapshot_matches(
                        &root.join("objects").join(&skill.bundle_digest).join("tree"),
                        &skill.bundle_digest,
                    )? {
                        return fail("内容校验失败，请重新导入后重试");
                    }
                }
                let mut previous = state
                    .skills
                    .iter()
                    .filter(|s| {
                        s.source_id == replacement.source_id
                            && s.id != replacement_id
                            && s.id != sid
                    })
                    .cloned()
                    .collect::<Vec<_>>();
                previous.push(old.clone());
                state
                    .content_backups
                    .retain(|b| b.source_id != replacement.source_id);
                state.content_backups.push(ContentBackup {
                    source_id: replacement.source_id.clone(),
                    created_at: now(),
                    skills: previous,
                    added_skill_ids: vec![],
                });
                let mut next = replacement.clone();
                next.id = old.id.clone();
                next.name = old.name;
                next.installed_at = old.installed_at;
                *state.skills.iter_mut().find(|s| s.id == sid).unwrap() = next;
                merge_records(
                    state,
                    &BTreeMap::from([(replacement_id.to_string(), sid.to_string())]),
                );
                Ok(())
            },
        )?;
        Ok(serde_json::to_value(state)?)
    }

    pub(crate) fn undo_content_update(&self, request: &Value) -> Result<Value> {
        let state = self.transact(
            "undo_content_update",
            "撤销上次来源更新",
            |state, root, changes| {
                if state.schema_version < 3 {
                    return fail("请先切换为单一内容模式");
                }
                if request["expectedRevision"].as_u64() != Some(state.revision as u64) {
                    return fail("资料库已变化，请重新确认");
                }
                let source_id = text(request, "sourceId")?;
                let backup = state
                    .content_backups
                    .iter()
                    .find(|b| b.source_id == source_id)
                    .cloned()
                    .ok_or_else(|| error::Error::Message("没有可恢复的上次更新".into()))?;
                let mut verified = BTreeSet::new();
                for skill in &backup.skills {
                    if verified.insert(skill.bundle_digest.clone())
                        && !files::snapshot_matches(
                            &root.join("objects").join(&skill.bundle_digest).join("tree"),
                            &skill.bundle_digest,
                        )?
                    {
                        return fail("恢复备份已变化，未执行撤销");
                    }
                    if let Some(current) = state.skills.iter_mut().find(|s| s.id == skill.id) {
                        // Preserve the visible identity; restore the complete source member payload.
                        current.source_id = skill.source_id.clone();
                        current.bundle_digest = skill.bundle_digest.clone();
                        current.relative_path = skill.relative_path.clone();
                        current.description = skill.description.clone();
                        current.version = skill.version.clone();
                        current.external_path = skill.external_path.clone();
                    }
                }
                let bindings = state
                    .bindings
                    .iter()
                    .filter(|b| backup.added_skill_ids.contains(&b.skill_id))
                    .cloned()
                    .collect::<Vec<_>>();
                for binding in bindings {
                    for claim in binding.claims {
                        operations::revoke(state, root, changes, &[binding.id.clone()], &claim)?;
                    }
                }
                state
                    .skills
                    .retain(|s| !backup.added_skill_ids.contains(&s.id));
                for preset in &mut state.presets {
                    preset
                        .skill_ids
                        .retain(|id| !backup.added_skill_ids.contains(id));
                }
                for subscription in &mut state.preset_packages {
                    subscription
                        .selected_ids
                        .retain(|id| !backup.added_skill_ids.contains(id));
                }
                Self::refresh_package_members(state);
                state.content_backups.retain(|b| b.source_id != source_id);
                for source in state.sources.iter_mut().filter(|s| {
                    s.id == source_id || backup.skills.iter().any(|skill| skill.source_id == s.id)
                }) {
                    if let Some(skill) = backup.skills.iter().find(|s| s.source_id == source.id) {
                        source.version = skill.bundle_digest.clone();
                    }
                    source.policy.mode = "notify".into();
                    source.status = "available".into();
                    source.next_check.clear();
                }
                Ok(())
            },
        )?;
        Ok(serde_json::to_value(state)?)
    }
}

pub(crate) fn library_directory(root: &Path) -> PathBuf {
    if root.file_name().is_some_and(|name| name == ".skilldock") {
        root.parent().unwrap().to_path_buf()
    } else {
        root.join("skills")
    }
}

pub(crate) fn plan_entries(
    before: &Snapshot,
    after: &mut Snapshot,
    root: &Path,
    changes: &mut Vec<Change>,
) -> Result<()> {
    if after.schema_version < 3 {
        return Ok(());
    }
    let directory = library_directory(root);
    fs::create_dir_all(&directory)?;
    let mut desired = BTreeMap::new();
    let mut names = BTreeSet::new();
    for skill in &after.skills {
        let name = files::clean_name(&skill.name)?;
        if name.starts_with('.') || !names.insert(name.to_lowercase()) {
            return fail("Skill 目录名称重复或为隐藏名称，请重命名");
        }
        desired.insert(skill.id.clone(), name);
    }
    // Adopt only exact generated legacy aliases during the explicit migration.
    if before.schema_version < 3 {
        for entry in fs::read_dir(&directory)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().to_string();
            let Ok(target) = fs::read_link(entry.path()) else {
                continue;
            };
            let owned = before.skills.iter().any(|skill| {
                let version: String = skill.version.chars().take(8).collect();
                (name == skill.name || name == format!("{}@{version}", skill.name))
                    && target == skill_path(root, skill)
            });
            if owned {
                let next = after
                    .skills
                    .iter()
                    .find(|skill| desired.get(&skill.id) == Some(&name))
                    .map(|skill| skill_path(root, skill));
                changes.push(Change {
                    path: entry.path(),
                    before: Some(target),
                    after: next,
                    backup: None,
                    restore: false,
                    backup_digest: None,
                });
            }
        }
    }
    for (sid, name) in &before.library_entries {
        let Some(skill) = before.skills.iter().find(|s| &s.id == sid) else {
            return fail("目录入口的归属记录缺失");
        };
        let path = directory.join(name);
        let previous = skill_path(root, skill);
        let next = after
            .skills
            .iter()
            .find(|s| &s.id == sid)
            .filter(|_| desired.get(sid) == Some(name))
            .map(|s| skill_path(root, s));
        if !files::exists(&path) {
            continue;
        }
        if fs::read_link(&path).ok() != Some(previous.clone()) {
            return fail(format!("库入口已被外部修改，未覆盖：{}", path.display()));
        }
        if next.as_ref() != Some(&previous) {
            changes.push(Change {
                path,
                before: Some(previous),
                after: next,
                backup: None,
                restore: false,
                backup_digest: None,
            });
        }
    }
    for skill in &after.skills {
        let name = &desired[&skill.id];
        let path = directory.join(name);
        if changes.iter().any(|c| c.path == path && c.after.is_some()) {
            continue;
        }
        if before.library_entries.get(&skill.id) == Some(name) && files::exists(&path) {
            continue;
        }
        if files::exists(&path) && !changes.iter().any(|c| c.path == path && c.after.is_none()) {
            return fail(format!("目录已被占用，未覆盖：{}", path.display()));
        }
        changes.push(Change {
            path,
            before: None,
            after: Some(skill_path(root, skill)),
            backup: None,
            restore: false,
            backup_digest: None,
        });
    }
    after.library_entries = desired;
    Ok(())
}
