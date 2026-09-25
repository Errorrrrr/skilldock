use crate::*;
use std::collections::BTreeSet;

fn conflicting_member(
    state: &Snapshot,
    source_id: &str,
    digest: &str,
    relative: &str,
    name: &str,
) -> bool {
    // A previously accepted independent member remains the same identity on sync.
    // Only a new member or a changed declaration needs another name decision.
    if state.skills.iter().any(|skill| {
        skill.source_id == source_id
            && skill.relative_path == relative
            && crate::declared_skill_name(Path::new(&state.storage_root), skill)
                .eq_ignore_ascii_case(name)
    }) {
        return false;
    }
    state.skills.iter().any(|skill| {
        if skill.source_id == source_id && skill.relative_path == relative {
            return false;
        }
        if skill.external_path.is_none()
            && skill.bundle_digest == digest
            && skill.relative_path == relative
        {
            return false;
        }
        let declared = crate::declared_skill_name(Path::new(&state.storage_root), skill);
        declared.eq_ignore_ascii_case(name)
    })
}

impl Engine {
    pub(crate) fn preview_local_source(&self, request: &Value) -> Result<Value> {
        let mut preview = self.preview_preset_folder(request)?;
        let root = PathBuf::from(text(&preview, "root")?);
        let state = self.snapshot()?;
        if preview["revision"].as_u64() != Some(state.revision as u64) {
            return fail("资料库已变化，请重新扫描");
        }
        let source_id = state
            .sources
            .iter()
            .find(|s| {
                (s.id == optional(request, "sourceId", "") && s.kind == "local_reference")
                    || (s.kind == "local_managed" && Path::new(&s.path) == root)
            })
            .map(|s| s.id.as_str())
            .unwrap_or("");
        let view = self.local_source_view(&root, true)?;
        let content = view.as_ref().map(|v| v.path()).unwrap_or(&root);
        let digest = files::content_digest(content)?;
        for item in preview["items"].as_array_mut().unwrap() {
            let relative = Path::new(text(item, "path")?)
                .strip_prefix(&root)
                .map_err(|_| {
                    error::Error::Message("成员不在扫描范围，请选择原始实体包目录".into())
                })?
                .to_string_lossy()
                .replace('\\', "/");
            let original = root.join(&relative);
            let (name, description) = files::metadata_with_fallback(
                &content.join(&relative),
                original
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("skill"),
            )?;
            item["name"] = json!(name);
            item["description"] = json!(description);
            item["sameName"] = json!(conflicting_member(
                &state,
                source_id,
                &digest,
                &relative,
                text(item, "name")?
            ));
        }
        if files::content_digest(content)? != digest {
            return fail("扫描期间来源内容发生变化，请重新扫描");
        }
        preview["contentDigest"] = json!(digest);
        Ok(preview)
    }

    pub(crate) fn local_source_skills(state: &Snapshot, source_id: &str) -> Result<Vec<Skill>> {
        let source = state
            .sources
            .iter()
            .find(|s| {
                s.id == source_id
                    && matches!(s.kind.as_str(), "local_reference" | "local_managed")
                    && s.updates_removed != Some(true)
            })
            .ok_or_else(|| error::Error::Message("本地来源不存在或已移除".into()))?;
        let ids = source.local_member_ids.clone().unwrap_or_else(|| {
            state
                .skills
                .iter()
                .filter(|s| s.source_id == source_id)
                .map(|s| s.id.clone())
                .collect()
        });
        ids.iter()
            .map(|id| {
                state
                    .skills
                    .iter()
                    .find(|s| {
                        &s.id == id
                            && (source.kind == "local_reference") == s.external_path.is_some()
                    })
                    .cloned()
                    .ok_or_else(|| error::Error::Message("本地来源成员已变化，请重新扫描".into()))
            })
            .collect()
    }

    pub(crate) fn save_local_source(&self, request: &Value) -> Result<Value> {
        let name = text(request, "name")?.trim();
        if name.is_empty() {
            return fail("请输入本地来源名称");
        }
        let expected_digest = request
            .get("contentDigest")
            .and_then(Value::as_str)
            .filter(|digest| !digest.is_empty())
            .ok_or_else(|| error::Error::Message("请先扫描本地来源，再确认保存当前内容".into()))?;
        let preview = self.preview_preset_folder(request)?;
        let root = PathBuf::from(text(&preview, "root")?);
        let selected: BTreeSet<_> = strings(request, "selectedPaths")?.into_iter().collect();
        let items: Vec<_> = preview["items"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|item| selected.contains(item["path"].as_str().unwrap_or_default()))
            .cloned()
            .collect();
        if items.len() != selected.len() {
            return fail("所选成员已变化，请重新扫描");
        }
        let mut source_id = String::new();
        let snapshot = self.transact(
            "save_local_source",
            "保存本地来源",
            |state, library, changes| {
                if request.get("revision").and_then(Value::as_u64) != Some(state.revision as u64) {
                    return fail("资料库已变化，请重新扫描");
                }
                if state.schema_version < 3 {
                    return fail("请先在统一目录设置中启用单份当前内容，再将本地来源复制入库");
                }
                if root.starts_with(library)
                    || library.starts_with(&root)
                    || state
                        .targets
                        .iter()
                        .any(|t| fs::canonicalize(&t.path).is_ok_and(|p| root.starts_with(p)))
                {
                    return fail("请选择原始源码文件夹，不能使用统一库或工具分发目录");
                }
                let legacy = state.sources.iter().find(|s| {
                    s.id == optional(request, "sourceId", "") && s.kind == "local_reference"
                }).cloned();
                let migrating = legacy.is_some();
                if migrating && !flag(request, "migrate") {
                    return fail("这是旧版本的本地引用，请确认复制到统一库后再保存；原始文件保留");
                }
                let managed = state
                    .sources
                    .iter()
                    .find(|s| s.kind == "local_managed" && Path::new(&s.path) == root)
                    .cloned();
                if migrating && managed.is_some() {
                    return fail("此目录已有统一入库的本地来源，请先检查分发关系并移除旧引用配置");
                }
                let existing = legacy.or(managed);
                if !optional(request, "sourceId", "").is_empty()
                    && !existing.as_ref().is_some_and(|s| {
                        s.id == text(request, "sourceId").unwrap_or_default()
                            && Path::new(&s.path) == root
                    })
                {
                    return fail("本地来源位置已变化，请重新打开配置；移动来源请另行添加");
                }
                if existing.is_none() && items.is_empty() {
                    return fail("请至少选择一个 Skill");
                }
                source_id = existing.as_ref().map(|s| s.id.clone()).unwrap_or_else(id);
                if migrating {
                    let members = Self::local_source_skills(state, &source_id)?;
                    if members.iter().any(|skill| skill.source_id != source_id)
                        || state.sources.iter().any(|other| {
                            other.id != source_id
                                && other.kind == "local_reference"
                                && other.updates_removed != Some(true)
                                && other.local_member_ids.as_ref().is_some_and(|ids| {
                                    state.skills.iter().any(|skill| {
                                        skill.source_id == source_id && ids.contains(&skill.id)
                                    })
                                })
                        })
                    {
                        return fail("成员仍被其他本地引用来源共享，不能只迁入其中一处；请先整理这些来源的成员关系");
                    }
                }
                // Keep the original package boundary so sibling resources remain available.
                // The original checkout is only an import source, never a distribution entity.
                let view = self.local_source_view(&root, true)?;
                let content = view.as_ref().map(|v| v.path()).unwrap_or(&root);
                if files::content_digest(content)? != expected_digest {
                    return fail("来源内容在预览后发生变化，请重新扫描并确认");
                }
                let digest = files::snapshot_current_content(library, content)?;
                if digest != expected_digest {
                    return fail("复制期间来源内容发生变化，请重新扫描并确认");
                }
                let claim = format!("source:{source_id}");
                let targets: Vec<_> = state
                    .bindings
                    .iter()
                    .filter(|b| b.claims.contains(&claim))
                    .map(|b| b.target_id.clone())
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect();
                let mut member_ids = vec![];
                let mut updates = vec![];
                let mut replaced_members = vec![];
                for item in &items {
                    let path = fs::canonicalize(text(item, "path")?)?;
                    if fs::canonicalize(text(item, "entryPath")?).ok().as_ref() != Some(&path)
                        || path.starts_with(library)
                        || library.starts_with(&path)
                        || files::protected(&path)
                    {
                        return fail("所选成员位置已变化或位于统一库内，请重新扫描原始目录");
                    }
                    let relative = path
                        .strip_prefix(&root)
                        .map_err(|_| error::Error::Message("成员不在扫描范围".into()))?
                        .to_string_lossy()
                        .replace('\\', "/");
                    let stored = library.join("objects").join(&digest).join("tree").join(&relative);
                    let (name, description) = files::metadata_with_fallback(
                        &stored, path.file_name().and_then(|name| name.to_str()).unwrap_or("skill"),
                    )?;
                    if !flag(request, "keepConflicts")
                        && conflicting_member(state, &source_id, &digest, &relative, &name)
                    {
                        return fail(format!("「{name}」与已有成员重名且内容或包结构不同，请明确选择独立保留同名成员后再保存"));
                    }
                    if let Some(skill) = state.skills.iter().find(|s| {
                        (s.external_path.is_none() || migrating)
                            && s.relative_path == relative
                            && (s.source_id == source_id
                                || (s.bundle_digest == digest
                                    && existing.as_ref().and_then(|s| s.local_member_ids.as_ref())
                                        .is_some_and(|ids| ids.contains(&s.id))))
                    }) {
                        member_ids.push(skill.id.clone());
                        if skill.source_id == source_id {
                            updates.push((skill.id.clone(), name, description));
                        }
                    } else {
                        let prior_member = existing.as_ref()
                            .and_then(|source| source.local_member_ids.as_ref())
                            .and_then(|ids| state.skills.iter().find(|skill| {
                                ids.contains(&skill.id) && skill.relative_path == relative
                            }))
                            .map(|skill| skill.id.clone());
                        let skill = Skill {
                            id: id(),
                            name,
                            description,
                            source_id: source_id.clone(),
                            external_path: None,
                            bundle_digest: digest.clone(),
                            relative_path: relative,
                            version: digest[..12].into(),
                            installed_at: now(),
                        };
                        if let Some(previous) = prior_member {
                            replaced_members.push(previous);
                        }
                        member_ids.push(skill.id.clone());
                        state.skills.push(skill);
                    }
                }
                // All surviving package members use the same content, including those
                // retained only by manual/preset claims. Missing members keep their last copy.
                for skill in state.skills.iter().filter(|s| s.source_id == source_id) {
                    if updates.iter().any(|(id, _, _)| id == &skill.id) {
                        continue;
                    }
                    let stored = library.join("objects").join(&digest).join("tree").join(&skill.relative_path);
                    let original = root.join(&skill.relative_path);
                    match files::metadata_with_fallback(&stored, original.file_name().and_then(|name| name.to_str()).unwrap_or("skill")) {
                        Ok((name, description)) => updates.push((skill.id.clone(), name, description)),
                        Err(_) if migrating => return fail(format!("旧成员「{}」在原目录中缺失，无法完整迁入；请先恢复该成员或整理旧记录", skill.name)),
                        Err(_) => {}
                    }
                }
                let replacements: Vec<_> = state.bindings.iter()
                    .filter(|binding| binding.claims.contains(&claim)
                        && replaced_members.contains(&binding.skill_id))
                    .map(|binding| binding.id.clone()).collect();
                let removed: Vec<_> = state
                    .bindings
                    .iter()
                    .filter(|b| b.claims.contains(&claim) && !member_ids.contains(&b.skill_id)
                        && !replacements.contains(&b.id))
                    .map(|b| b.id.clone())
                    .collect();
                crate::operations::revoke(state, library, changes, &removed, &claim)?;
                let mut source = existing.unwrap_or(Source {
                    id: source_id.clone(),
                    name: name.into(),
                    kind: "local_managed".into(),
                    path: root.display().to_string(),
                    url: String::new(),
                    scan_subdir: String::new(),
                    reference: String::new(),
                    version: digest.clone(),
                    policy: Policy::default(),
                    last_checked: String::new(),
                    next_check: String::new(),
                    status: "current".into(),
                    error: String::new(),
                    updates_removed: None,
                    local_member_ids: None,
                });
                source.name = name.into();
                source.kind = "local_managed".into();
                source.local_member_ids = Some(member_ids.clone());
                source.version = digest.clone();
                source.status = "current".into();
                source.error.clear();
                source.last_checked = now();
                source.updates_removed = None;
                source.policy = Policy::default();
                source.next_check.clear();
                state.sources.retain(|s| s.id != source_id);
                state.sources.push(source);
                Self::observe_installations(state);
                let skills = state.skills.iter().filter(|s| member_ids.contains(&s.id)).cloned().collect::<Vec<_>>();
                if !targets.is_empty() && !skills.is_empty() {
                    crate::operations::distribute(
                        state,
                        library,
                        changes,
                        &skills,
                        &targets,
                        &claim,
                        false,
                        false,
                        &replacements,
                    )?;
                }
                // Add/revoke source claims using the existing current content first.
                // Finalization then changes every retained binding atomically, including
                // bindings also held by a manual action or a preset.
                for (skill_id, name, description) in updates {
                    let skill = state.skills.iter_mut().find(|s| s.id == skill_id).unwrap();
                    skill.name = name;
                    skill.description = description;
                    skill.external_path = None;
                    skill.bundle_digest = digest.clone();
                    skill.version = digest[..12].into();
                }
                Ok(())
            },
        )?;
        Ok(json!({"snapshot":snapshot,"sourceId":source_id}))
    }

    pub(crate) fn manage_local_source(&self, request: &Value) -> Result<Value> {
        let action = text(request, "action")?;
        let source_id = text(request, "sourceId")?;
        let state = self.transact(
            action,
            match action {
                "apply_local_source" => "整体分发本地来源",
                "revoke_local_source" => "取消本地来源分发",
                _ => "移除本地来源配置",
            },
            |state, library, changes| {
                if request.get("expectedRevision").and_then(Value::as_u64)
                    != Some(state.revision as u64)
                {
                    return fail("资料库已变化，请重新预览或确认");
                }
                let claim = format!("source:{source_id}");
                if action == "apply_local_source" {
                    let skills = Self::local_source_skills(state, source_id)?;
                    crate::operations::distribute(
                        state,
                        library,
                        changes,
                        &skills,
                        &strings(request, "targetIds")?,
                        &claim,
                        flag(request, "takeover"),
                        flag(request, "adoptExisting"),
                        &strings(request, "replaceBindingIds")?,
                    )?;
                } else {
                    // Cleanup only needs the source identity and claim. A missing member
                    // must not trap a stale source configuration in the library.
                    if !state.sources.iter().any(|s| {
                        s.id == source_id
                            && matches!(s.kind.as_str(), "local_reference" | "local_managed")
                    }) {
                        return fail("本地来源不存在");
                    }
                    let targets = strings(request, "targetIds")?;
                    let ids = state
                        .bindings
                        .iter()
                        .filter(|b| {
                            b.claims.contains(&claim)
                                && (action == "remove_local_source"
                                    || targets.contains(&b.target_id))
                        })
                        .map(|b| b.id.clone())
                        .collect::<Vec<_>>();
                    crate::operations::revoke(state, library, changes, &ids, &claim)?;
                    if action == "remove_local_source" {
                        let source = state
                            .sources
                            .iter_mut()
                            .find(|s| s.id == source_id)
                            .unwrap();
                        source.updates_removed = Some(true);
                        source.local_member_ids = Some(vec![]);
                    }
                }
                Ok(())
            },
        )?;
        Ok(serde_json::to_value(state)?)
    }
}
