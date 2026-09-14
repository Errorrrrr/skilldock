use crate::*;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Serialize, Deserialize)]
struct GitPreview {
    revision: u32,
    source: Source,
    digest: String,
    tree: PathBuf,
    #[serde(default)]
    issues: Vec<String>,
}

fn git_input(request: &Value) -> Result<(String, String, String)> {
    let input = text(request, "url")?.trim();
    let mut repository = input.to_string();
    let mut reference = optional(request, "reference", "HEAD").to_string();
    let mut subdir = optional(request, "subdir", "").to_string();
    if let Ok(mut url) = reqwest::Url::parse(input) {
        let parts: Vec<_> = url
            .path()
            .trim_matches('/')
            .split('/')
            .map(str::to_string)
            .collect();
        let tree =
            if url.host_str() == Some("github.com") && parts.get(2).is_some_and(|p| p == "tree") {
                Some((2, 3))
            } else {
                parts
                    .windows(2)
                    .position(|p| p[0] == "-" && p[1] == "tree")
                    .map(|i| (i, i + 2))
            };
        if let Some((repo_end, branch_start)) = tree {
            if repo_end < 2 || parts.len() <= branch_start {
                return fail("Git 子目录链接不完整");
            }
            let repo_path = format!(
                "/{}.git",
                parts[..repo_end].join("/").trim_end_matches(".git")
            );
            url.set_path(&repo_path);
            repository = url.to_string();
            let tail = parts[branch_start..].join("/");
            let inferred = if reference != "HEAD"
                && !reference.is_empty()
                && (tail == reference || tail.starts_with(&format!("{reference}/")))
            {
                reference.clone()
            } else {
                parts[branch_start].clone()
            };
            if reference == "HEAD" || reference.is_empty() {
                reference = inferred.clone();
            }
            if subdir.is_empty() {
                subdir = tail
                    .strip_prefix(&inferred)
                    .unwrap_or("")
                    .trim_start_matches('/')
                    .into();
            }
        }
    }
    if reference.is_empty() {
        reference = "HEAD".into();
    }
    Ok((repository, reference, subdir))
}

impl Engine {
    pub(crate) async fn preview_git_package(&self, request: &Value) -> Result<Value> {
        let baseline = self.snapshot()?;
        if !baseline.initialized {
            return fail("请先设置统一目录");
        }
        let root = PathBuf::from(&baseline.storage_root);
        let (url, reference, subdir) = git_input(request)?;
        let prepared = network::prepare_git(&root.join("cache"), &url, &reference, &subdir).await?;
        let source = prepared_info(&prepared);
        let issues = files::python_environment_issues(&prepared.path)?;
        let view = Self::remote_source_view(&prepared.path)?;
        let path = view.as_ref().map(|v| v.path()).unwrap_or(&prepared.path);
        let token = id();
        let directory = root.join("cache").join(format!("preview-{token}"));
        let tree = directory.join("tree");
        let digest = files::content_digest(path)?;
        files::copy_tree(path, &tree)?;
        if files::content_digest(&tree)? != digest {
            return fail("预览期间内容发生变化，请重试");
        }
        let state = self.snapshot()?;
        if state.storage_root != baseline.storage_root {
            return fail("预览期间统一目录已变化，请重试");
        }
        let existing = state.sources.iter().find(|s| {
            s.kind == "git"
                && s.url == source.url
                && s.reference == source.reference
                && s.scan_subdir == source.scan_subdir
        });
        let items: Vec<_> = files::scan(&tree.join(&source.scan_subdir))?.items.into_iter().filter(|i|i.status=="ready").map(|item| {
            let relative = Path::new(&item.path).strip_prefix(&tree).unwrap().to_string_lossy().replace('\\', "/");
            let member = existing.and_then(|src|state.skills.iter().find(|s|s.source_id==src.id && s.relative_path==relative));
            let change = match member {
                None => "added",
                Some(s) if files::content_digest(&skill_path(&root,s)).ok().is_some_and(|d|files::content_digest(Path::new(&item.path)).ok()==Some(d)) => "unchanged",
                _ => "changed",
            };
            json!({"path":relative,"name":item.name,"description":item.description,"existingId":member.map(|s|&s.id),"change":change})
        }).collect();
        if items.is_empty() && existing.is_none() {
            return fail("Git 子目录中未发现 Skill");
        }
        let removed: Vec<_> = state
            .skills
            .iter()
            .filter(|s| {
                existing.is_some_and(|src| s.source_id == src.id)
                    && !items.iter().any(|i| i["path"] == s.relative_path)
            })
            .map(|s| json!({"skillId":s.id,"name":s.name,"path":s.relative_path}))
            .collect();
        files::atomic_json(
            &directory.join("preview.json"),
            &GitPreview {
                revision: state.revision,
                source: source.clone(),
                digest: digest.clone(),
                tree,
                issues: issues.clone(),
            },
        )?;
        Ok(
            json!({"token":token,"revision":state.revision,"url":source.url,"reference":source.reference,"subdir":source.scan_subdir,"commit":source.version,"digest":digest,"name":source.name,"items":items,"removed":removed,"issues":issues}),
        )
    }

    pub(crate) fn import_git_package(&self, request: &Value) -> Result<Value> {
        let root = self
            .root()?
            .ok_or_else(|| error::Error::Message("请先设置统一目录".into()))?;
        let token = text(request, "token")?;
        uuid::Uuid::parse_str(token).map_err(|_| error::Error::Message("预览标识无效".into()))?;
        let directory = root.join("cache").join(format!("preview-{token}"));
        let preview: GitPreview =
            serde_json::from_slice(&fs::read(directory.join("preview.json"))?)?;
        let tree = directory.join("tree");
        files::safe_relative(&preview.source.scan_subdir)?;
        let removals = strings(request, "removedIds")?;
        if preview.tree != tree
            || fs::canonicalize(&tree)? != tree
            || files::content_digest(&tree)? != preview.digest
        {
            return fail("预览内容已变化，请重新预览");
        }
        let entries: BTreeMap<_, _> = files::scan(&tree.join(&preview.source.scan_subdir))?
            .items
            .into_iter()
            .filter(|i| i.status == "ready")
            .map(|i| {
                (
                    Path::new(&i.path)
                        .strip_prefix(&tree)
                        .unwrap()
                        .to_string_lossy()
                        .replace('\\', "/"),
                    i,
                )
            })
            .collect();
        let selected = if request.get("selectedPaths").is_some() {
            strings(request, "selectedPaths")?
        } else {
            entries.keys().cloned().collect()
        };
        if selected.iter().any(|p| !entries.contains_key(p)) {
            return fail("所选成员不在预览范围，请重新预览");
        }
        let selected: BTreeSet<_> = selected.into_iter().collect();
        let mut package_id = String::new();
        let mut selected_ids = vec![];
        let mut preset_id = String::new();
        self.transact(
            "import_git_package",
            "同步 Git Skill 集合",
            |s, root, _| {
                if s.revision != preview.revision
                    || request.get("revision").and_then(Value::as_u64) != Some(s.revision as u64)
                {
                    return fail("资料库已变化，请重新预览");
                }
                if files::snapshot_current_content(root, &tree)? != preview.digest {
                    return fail("预览内容已变化，请重新预览");
                }
                let mut source = preview.source.clone();
                if let Some(old) = s.sources.iter().find(|x| {
                    x.kind == "git"
                        && x.url == source.url
                        && x.reference == source.reference
                        && x.scan_subdir == source.scan_subdir
                }) {
                    source.id = old.id.clone();
                    source.policy = old.policy.clone();
                    source.path = old.path.clone();
                    source.next_check = old.next_check.clone();
                }
                let source_id = source.id.clone();
                let mut missing = vec![];
                for member in s.skills.iter_mut().filter(|m| m.source_id == source_id) {
                    if let Some(item) = entries.get(&member.relative_path) {
                        member.bundle_digest = preview.digest.clone();
                        member.version = source.version.chars().take(12).collect();
                        member.name = item.name.clone();
                        member.description = item.description.clone();
                    } else {
                        missing.push(member.id.clone());
                    }
                }
                for (relative, item) in &entries {
                    if selected.contains(relative)
                        && !s
                            .skills
                            .iter()
                            .any(|m| m.source_id == source_id && m.relative_path == *relative)
                    {
                        s.skills.push(Skill {
                            id: id(),
                            name: item.name.clone(),
                            description: item.description.clone(),
                            source_id: source_id.clone(),
                            bundle_digest: preview.digest.clone(),
                            relative_path: relative.clone(),
                            version: source.version.chars().take(12).collect(),
                            installed_at: now(),
                            external_path: None,
                        });
                    }
                }
                if removals.iter().any(|id| !missing.contains(id)) {
                    return fail("移除成员已变化，请重新预览");
                }
                if !missing.is_empty() {
                    source.status = "attention".into();
                    source.error = "部分成员已从来源移除，保留旧安装，请在预设中确认处理".into();
                }
                s.sources.retain(|x| x.id != source_id);
                s.sources.push(source.clone());
                package_id = s
                    .packages
                    .iter()
                    .find(|p| p.remote && p.scopes.iter().any(|scope| scope.source_id == source_id))
                    .map(|p| p.id.clone())
                    .unwrap_or_else(id);
                s.packages.retain(|p| p.id != package_id);
                s.packages.push(SkillPackage {
                    remote: true,
                    missing_member_ids: missing,
                    issues: preview.issues.clone(),
                    id: package_id.clone(),
                    name: source.name.clone(),
                    path: source.url.clone(),
                    scopes: vec![PackageScope {
                        auto_add: Some(flag(request, "autoAdd")),
                        origin_path: String::new(),
                        source_id: source_id.clone(),
                        prefix: source.scan_subdir.clone(),
                        excluded: entries
                            .keys()
                            .filter(|relative| {
                                !selected.contains(*relative)
                                    && !s.skills.iter().any(|skill| {
                                        skill.source_id == source_id
                                            && &skill.relative_path == *relative
                                    })
                            })
                            .cloned()
                            .collect(),
                    }],
                    member_ids: vec![],
                });
                Self::update_package_presets(s, &source_id);
                selected_ids = s
                    .skills
                    .iter()
                    .filter(|m| m.source_id == source_id && selected.contains(&m.relative_path))
                    .map(|m| m.id.clone())
                    .collect();
                let requested_preset = optional(request, "presetId", "");
                let name = optional(request, "presetName", "").trim();
                if !requested_preset.is_empty() || !name.is_empty() {
                    if !requested_preset.is_empty() && !name.is_empty() {
                        return fail("请选择已有预设或新建预设");
                    }
                    if selected_ids.is_empty() && requested_preset.is_empty() {
                        return fail("请至少选择一个 Skill 加入预设");
                    }
                    let mut preset = if requested_preset.is_empty() {
                        Preset {
                            id: id(),
                            name: name.into(),
                            description: String::new(),
                            skill_ids: vec![],
                            revision: 0,
                            locks: BTreeMap::new(),
                        }
                    } else {
                        s.presets
                            .iter()
                            .find(|p| p.id == requested_preset)
                            .cloned()
                            .ok_or_else(|| error::Error::Message("预设不存在".into()))?
                    };
                    preset_id = preset.id.clone();
                    let package = s.packages.iter().find(|p| p.id == package_id).unwrap();
                    let old = s
                        .preset_packages
                        .iter()
                        .find(|p| p.preset_id == preset_id && p.package_id == package_id)
                        .map(|p| p.selected_ids.clone())
                        .unwrap_or_default();
                    let mut chosen = selected_ids.clone();
                    chosen.extend(
                        old.iter()
                            .filter(|id| {
                                package.missing_member_ids.contains(id) && !removals.contains(id)
                            })
                            .cloned(),
                    );
                    chosen.sort();
                    chosen.dedup();
                    let others: BTreeSet<_> = s
                        .preset_packages
                        .iter()
                        .filter(|p| p.preset_id == preset_id && p.package_id != package_id)
                        .flat_map(|p| p.selected_ids.iter().cloned())
                        .collect();
                    preset.skill_ids.retain(|id| {
                        ((!old.contains(id) || chosen.contains(id)) && !removals.contains(id))
                            || others.contains(id)
                    });
                    for id in &chosen {
                        if !preset.skill_ids.contains(id) {
                            preset.skill_ids.push(id.clone());
                        }
                    }
                    preset.locks.retain(|id, _| preset.skill_ids.contains(id));
                    for id in &chosen {
                        let member = s
                            .skills
                            .iter()
                            .find(|m| &m.id == id)
                            .cloned()
                            .ok_or_else(|| error::Error::Message("预设成员不存在".into()))?;
                        preset.locks.insert(id.clone(), member);
                    }
                    preset.revision += 1;
                    let excluded_ids = package
                        .member_ids
                        .iter()
                        .filter(|id| !chosen.contains(id))
                        .cloned()
                        .collect();
                    s.preset_packages
                        .retain(|p| !(p.preset_id == preset_id && p.package_id == package_id));
                    s.preset_packages.push(PresetPackage {
                        preset_id: preset_id.clone(),
                        package_id: package_id.clone(),
                        auto_add: request
                            .get("autoAdd")
                            .and_then(Value::as_bool)
                            .unwrap_or(true),
                        excluded_ids,
                        selected_ids: chosen,
                    });
                    s.presets.retain(|p| p.id != preset_id);
                    s.presets.push(preset);
                }
                Ok(())
            },
        )?;
        self.reconcile_packages()?;
        Ok(
            json!({"snapshot":self.snapshot()?,"packageId":package_id,"skillIds":selected_ids,"presetId":preset_id}),
        )
    }
}

#[cfg(test)]
mod source_link_tests {
    use super::*;
    #[test]
    fn gitlab_subgroups_and_explicit_branch_are_preserved() {
        assert_eq!(
            git_input(
                &json!({"url":"https://git.example.com/group/team/skills/-/tree/main/skills"})
            )
            .unwrap(),
            (
                "https://git.example.com/group/team/skills.git".into(),
                "main".into(),
                "skills".into()
            )
        );
        assert_eq!(git_input(&json!({"url":"https://gitlab.com/group/repo/-/tree/feature/test/skills", "reference":"feature/test"})).unwrap(),
            ("https://gitlab.com/group/repo.git".into(), "feature/test".into(), "skills".into()));
        assert_eq!(git_input(&json!({"url":"git@git.example.com:group/team/repo.git", "reference":"main", "subdir":"skills"})).unwrap(),
            ("git@git.example.com:group/team/repo.git".into(), "main".into(), "skills".into()));
        assert_eq!(
            git_input(&json!({"url":"https://github.com/group/repo/tree/main/skills"})).unwrap(),
            (
                "https://github.com/group/repo.git".into(),
                "main".into(),
                "skills".into()
            )
        );
    }
}
