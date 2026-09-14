use crate::*;
use std::collections::BTreeSet;

impl Engine {
    pub(crate) fn local_source_skills(state: &Snapshot, source_id: &str) -> Result<Vec<Skill>> {
        let source = state
            .sources
            .iter()
            .find(|s| {
                s.id == source_id && s.kind == "local_reference" && s.updates_removed != Some(true)
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
                    .find(|s| &s.id == id && s.external_path.is_some())
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
                if root.starts_with(library)
                    || library.starts_with(&root)
                    || state
                        .targets
                        .iter()
                        .any(|t| fs::canonicalize(&t.path).is_ok_and(|p| root.starts_with(p)))
                {
                    return fail("请选择原始源码文件夹，不能使用统一库或工具分发目录");
                }
                let existing = state
                    .sources
                    .iter()
                    .find(|s| s.kind == "local_reference" && Path::new(&s.path) == root)
                    .cloned();
                if !optional(request, "sourceId", "").is_empty()
                    && existing.as_ref().map(|s| s.id.as_str()) != Some(text(request, "sourceId")?)
                {
                    return fail("本地来源位置已变化，请重新打开配置；移动来源请另行添加");
                }
                if existing.is_none() && items.is_empty() {
                    return fail("请至少选择一个 Skill");
                }
                source_id = existing.as_ref().map(|s| s.id.clone()).unwrap_or_else(id);
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
                for item in &items {
                    let path = fs::canonicalize(text(item, "path")?)?;
                    if fs::canonicalize(text(item, "entryPath")?).ok().as_ref() != Some(&path)
                        || path.starts_with(library)
                        || library.starts_with(&path)
                        || files::protected(&path)
                    {
                        return fail("所选成员位置已变化或位于统一库内，请重新扫描原始目录");
                    }
                    let (name, description) = files::metadata(&path)?;
                    if let Some(skill) = state.skills.iter_mut().find(|s| {
                        s.external_path
                            .as_ref()
                            .is_some_and(|p| fs::canonicalize(p).ok().as_ref() == Some(&path))
                    }) {
                        skill.name = name;
                        skill.description = description;
                        member_ids.push(skill.id.clone());
                    } else {
                        let skill = Skill {
                            id: id(),
                            name,
                            description,
                            source_id: source_id.clone(),
                            external_path: Some(path.display().to_string()),
                            bundle_digest: String::new(),
                            relative_path: Path::new(text(item, "entryPath")?)
                                .strip_prefix(&root)
                                .map_err(|_| error::Error::Message("成员不在扫描范围".into()))?
                                .to_string_lossy()
                                .replace('\\', "/"),
                            version: "跟随本地内容".into(),
                            installed_at: now(),
                        };
                        member_ids.push(skill.id.clone());
                        state.skills.push(skill);
                    }
                }
                let removed: Vec<_> = state
                    .bindings
                    .iter()
                    .filter(|b| b.claims.contains(&claim) && !member_ids.contains(&b.skill_id))
                    .map(|b| b.id.clone())
                    .collect();
                crate::operations::revoke(state, library, changes, &removed, &claim)?;
                let mut source = existing.unwrap_or(Source {
                    id: source_id.clone(),
                    name: name.into(),
                    kind: "local_reference".into(),
                    path: root.display().to_string(),
                    url: String::new(),
                    scan_subdir: String::new(),
                    reference: String::new(),
                    version: "跟随本地内容".into(),
                    policy: Policy::default(),
                    last_checked: String::new(),
                    next_check: String::new(),
                    status: "local_reference".into(),
                    error: String::new(),
                    updates_removed: None,
                    local_member_ids: None,
                });
                source.name = name.into();
                source.local_member_ids = Some(member_ids);
                source.updates_removed = None;
                source.policy = Policy::default();
                source.next_check.clear();
                state.sources.retain(|s| s.id != source_id);
                state.sources.push(source);
                Self::observe_installations(state);
                let skills = Self::local_source_skills(state, &source_id)?;
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
                        &[],
                    )?;
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
                let skills = Self::local_source_skills(state, source_id)?;
                let claim = format!("source:{source_id}");
                if action == "apply_local_source" {
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
