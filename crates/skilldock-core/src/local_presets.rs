use crate::*;
use std::collections::{BTreeMap, BTreeSet};

impl Engine {
    pub(crate) fn preview_preset_folder(&self, request: &Value) -> Result<Value> {
        let state = self.snapshot()?;
        let root = fs::canonicalize(files::absolute(text(request, "path")?)?)?;
        if files::protected(&root) {
            return fail("该目录由工具内部管理，不能作为预设来源");
        }
        let excluded = files::local_import_exclusions(&root)?;
        let scanned = files::scan(&root)?;
        let mut targets = state.targets.clone();
        targets.extend(self.discover_configured()?);
        let mut links: BTreeMap<PathBuf, Vec<String>> = BTreeMap::new();
        let mut visited = BTreeSet::new();
        for target in targets {
            let path = PathBuf::from(target.path);
            if !visited.insert(path.clone()) || !path.is_dir() {
                continue;
            }
            // Agent installations are direct entries; never follow a package's
            // links recursively into arbitrary external directories.
            let Ok(entries) = fs::read_dir(path) else {
                continue;
            };
            for entry in entries.flatten() {
                if !entry.file_type().is_ok_and(|kind| kind.is_symlink()) {
                    continue;
                }
                if let Ok(actual) = fs::canonicalize(entry.path()) {
                    links
                        .entry(actual)
                        .or_default()
                        .push(entry.path().display().to_string());
                }
            }
        }
        let mut items = vec![];
        let mut seen = BTreeSet::new();
        for item in scanned.items {
            let path = PathBuf::from(&item.path);
            if excluded.iter().any(|excluded| path.starts_with(excluded)) {
                continue;
            }
            if !matches!(item.status.as_str(), "ready" | "linked") {
                continue;
            }
            let Ok(actual) = fs::canonicalize(&path) else {
                continue;
            };
            if files::protected(&actual) || !seen.insert(actual.clone()) {
                continue;
            }
            let Ok((name, description)) = files::metadata(&actual) else {
                continue;
            };
            let existing = state.skills.iter().find(|skill| {
                fs::canonicalize(skill_path(Path::new(&state.storage_root), skill))
                    .ok()
                    .as_ref()
                    == Some(&actual)
            });
            // Package scopes retain each checkout's original directory, including
            // Git sources, nested repositories and multiple clones of one source.
            let snapshot_member = state.skills.iter().find(|skill| {
                skill.external_path.is_none()
                    && (state.packages.iter().any(|package| {
                        package.member_ids.contains(&skill.id)
                            && package.scopes.iter().any(|scope| {
                                scope.source_id == skill.source_id
                                    && !scope.origin_path.is_empty()
                                    && fs::canonicalize(
                                        Path::new(&scope.origin_path).join(&skill.relative_path),
                                    )
                                    .ok()
                                    .as_ref()
                                        == Some(&actual)
                            })
                    }) || state.sources.iter().any(|source| {
                        source.id == skill.source_id
                            && matches!(source.kind.as_str(), "local" | "git")
                            && !source.path.is_empty()
                            && fs::canonicalize(Path::new(&source.path).join(&skill.relative_path))
                                .ok()
                                .as_ref()
                                == Some(&actual)
                    }))
            });
            let mut existing_links = vec![];
            for (entity, entries) in &links {
                if let Ok(suffix) = actual.strip_prefix(entity) {
                    for entry in entries {
                        existing_links.push(if suffix.as_os_str().is_empty() {
                            entry.clone()
                        } else {
                            Path::new(entry).join(suffix).display().to_string()
                        });
                    }
                }
            }
            existing_links.sort();
            existing_links.dedup();
            let conflict = state.skills.iter().any(|skill| {
                skill.name == name
                    && Some(&skill.id) != existing.map(|s| &s.id)
                    && Some(&skill.id) != snapshot_member.map(|s| &s.id)
            });
            let library = Path::new(&state.storage_root);
            let error = if existing.is_none()
                && (actual.starts_with(library) || library.starts_with(&actual))
            {
                "不能把统一库内部实体登记为外部引用；请从 Skill 库选择已有成员"
            } else {
                ""
            };
            let change = if let Some(member) = snapshot_member.or(existing) {
                let stored = skill_path(Path::new(&state.storage_root), member);
                if matches!((files::content_digest(&actual), files::content_digest(&stored)), (Ok(a), Ok(b)) if a == b)
                {
                    "unchanged"
                } else {
                    "changed"
                }
            } else {
                "added"
            };
            items.push(json!({"change":change,"error":error,"path":actual,"entryPath":path,"name":name,"description":description,"existingId":existing.map(|s|&s.id),"snapshotId":snapshot_member.map(|s|&s.id),"existingLinks":existing_links,"sameName":conflict}));
        }
        items.sort_by_key(|item| item["path"].as_str().unwrap_or_default().to_owned());
        let package = state.packages.iter().find(|p| Path::new(&p.path) == root);
        let removed: Vec<_> = package
            .into_iter()
            .flat_map(|p| p.member_ids.iter())
            .filter_map(|id| {
                let member = state.skills.iter().find(|s| &s.id == id)?;
                if items.iter().any(|i| {
                    i["snapshotId"].as_str() == Some(id.as_str())
                        || i["existingId"].as_str() == Some(id.as_str())
                }) {
                    return None;
                }
                Some(json!({"skillId":id,"name":member.name,"path":member.relative_path}))
            })
            .collect();
        Ok(
            json!({"packageId":package.map(|p|&p.id),"removed":removed,"root":root,"revision":state.revision,"items":items,"warnings":scanned.warnings}),
        )
    }

    pub(crate) fn import_preset_folder(&self, request: &Value) -> Result<Value> {
        let mode = text(request, "mode")?;
        if !matches!(mode, "reference" | "copy") {
            return fail("请选择引用本地包或复制到统一库");
        }
        let preview = self.preview_preset_folder(request)?;
        let root = PathBuf::from(text(&preview, "root")?);
        let mut selected = BTreeSet::new();
        for path in strings(request, "selectedPaths")? {
            selected.insert(
                fs::canonicalize(files::absolute(&path)?)?
                    .display()
                    .to_string(),
            );
        }
        if selected.is_empty() {
            return fail("请至少选择一个 Skill");
        }
        let items: Vec<_> = preview["items"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|item| selected.contains(item["path"].as_str().unwrap_or_default()))
            .cloned()
            .collect();
        if items.len() != selected.len() {
            return fail("部分所选 Skill 已变化或不在可导入范围，请重新扫描");
        }
        let mut member_ids = vec![];
        let state = self.transact(
            "preset_members",
            "添加本地包成员",
            |state, library, changes| {
                if request.get("revision").and_then(Value::as_u64) != Some(state.revision as u64) {
                    return fail("资料库已变化，请重新扫描后导入");
                }
                if root.starts_with(library) || library.starts_with(&root) {
                    return fail("请从 Skill 库直接选择已入库成员，不要重新导入统一库目录");
                }
                if mode == "copy" {
                    if items.iter().any(|item| {
                        !Path::new(item["path"].as_str().unwrap()).starts_with(&root)
                            || item["entryPath"] != item["path"]
                    }) {
                        return fail(
                            "所选成员是已有安装软链，请选择引用本地包；复制请扫描原始实体包目录",
                        );
                    }
                    self.prepare_import(
                        &root,
                        selected.iter().cloned().collect(),
                        false,
                        None,
                        state,
                        library,
                        changes,
                    )?;
                    for item in &items {
                        let relative = Path::new(text(item, "path")?)
                            .strip_prefix(&root)
                            .unwrap()
                            .to_string_lossy()
                            .replace('\\', "/");
                        let skill = state
                            .skills
                            .iter()
                            .find(|skill| {
                                skill.external_path.is_none()
                                    && skill.relative_path == relative
                                    && state.sources.iter().any(|source| {
                                        source.id == skill.source_id
                                            && source.kind == "local"
                                            && Path::new(&source.path) == root
                                    })
                            })
                            .ok_or_else(|| {
                                error::Error::Message("导入成员匹配失败，请重新扫描".into())
                            })?;
                        member_ids.push(skill.id.clone());
                    }
                } else {
                    let source_id = if let Some(source) = state.sources.iter().find(|source| {
                        source.kind == "local_reference" && Path::new(&source.path) == root
                    }) {
                        source.id.clone()
                    } else {
                        let source_id = id();
                        state.sources.push(Source {
                            updates_removed: None,
                            local_member_ids: None,
                            id: source_id.clone(),
                            name: root
                                .file_name()
                                .unwrap_or_default()
                                .to_string_lossy()
                                .into(),
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
                        });
                        source_id
                    };
                    for item in &items {
                        let path = fs::canonicalize(text(item, "path")?)?;
                        if fs::canonicalize(text(item, "entryPath")?).ok().as_ref() != Some(&path)
                            || files::protected(&path)
                        {
                            return fail("所选目录位置发生变化，请重新扫描");
                        }
                        let (name, description) = files::metadata(&path)?;
                        if let Some(skill) = state.skills.iter().find(|skill| {
                            fs::canonicalize(skill_path(library, skill)).ok().as_ref()
                                == Some(&path)
                        }) {
                            member_ids.push(skill.id.clone());
                            continue;
                        }
                        if path.starts_with(library) || library.starts_with(&path) {
                            return fail(
                                "不能把统一库内部实体登记为外部引用；请从 Skill 库选择已有成员",
                            );
                        }
                        let skill = Skill {
                            external_path: Some(path.display().to_string()),
                            id: id(),
                            name,
                            description,
                            source_id: source_id.clone(),
                            bundle_digest: String::new(),
                            relative_path: Path::new(text(item, "entryPath")?)
                                .strip_prefix(&root)
                                .unwrap()
                                .to_string_lossy()
                                .replace('\\', "/"),
                            version: "跟随本地内容".into(),
                            installed_at: now(),
                        };
                        member_ids.push(skill.id.clone());
                        state.skills.push(skill);
                    }
                    // Register existing links as borrowed manual installations. Their
                    // ownership remains external; cancellation never removes them.
                    for (item, skill_id) in items.iter().zip(&member_ids) {
                        let skill = state
                            .skills
                            .iter()
                            .find(|skill| &skill.id == skill_id)
                            .unwrap()
                            .clone();
                        if skill.external_path.is_none() {
                            continue;
                        }
                        for link in item["existingLinks"].as_array().unwrap() {
                            let path = PathBuf::from(link.as_str().unwrap());
                            if state
                                .bindings
                                .iter()
                                .any(|binding| Path::new(&binding.path) == path)
                                || !path.ancestors().any(|entry| {
                                    fs::symlink_metadata(entry)
                                        .is_ok_and(|meta| meta.file_type().is_symlink())
                                })
                                || fs::canonicalize(&path).ok()
                                    != fs::canonicalize(skill_path(library, &skill)).ok()
                            {
                                continue;
                            }
                            let parent = path.parent().unwrap();
                            let target_id = if let Some(target) = state
                                .targets
                                .iter()
                                .find(|target| Path::new(&target.path) == parent)
                            {
                                target.id.clone()
                            } else {
                                let target = crate::operations::inferred_target(
                                    parent,
                                    &state.settings.agent_profiles,
                                );
                                let target_id = target.id.clone();
                                state.targets.push(target);
                                target_id
                            };
                            state.bindings.push(Binding {
                                original_link: None,
                                id: id(),
                                skill_id: skill.id.clone(),
                                target_id,
                                path: path.display().to_string(),
                                version: skill.version.clone(),
                                digest: String::new(),
                                relative_path: skill.relative_path.clone(),
                                claims: vec!["manual".into()],
                                follow: false,
                                external_path: skill.external_path.clone(),
                                borrowed: true,
                            });
                        }
                    }
                    // Reusing every member of another registered root needs no empty source.
                    state.sources.retain(|source| {
                        source.id != source_id
                            || source.local_member_ids.is_some()
                            || state
                                .skills
                                .iter()
                                .any(|skill| skill.source_id == source_id)
                    });
                }
                Ok(())
            },
        )?;
        Ok(json!({"snapshot":state,"skillIds":member_ids}))
    }
}
