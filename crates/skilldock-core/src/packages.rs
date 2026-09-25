use crate::*;
use std::collections::{BTreeMap, BTreeSet};

fn within(path: &str, prefix: &str) -> bool {
    prefix.is_empty() || path == prefix || path.starts_with(&format!("{prefix}/"))
}
fn git_text(path: &Path, args: &[&str]) -> Option<String> {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(path)
        .args(args)
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
}
impl Engine {
    // Refresh cached legacy warnings only when both the original and stored Skill
    // have a recognizable rebuild entrypoint. This never installs dependencies.
    pub(crate) fn refresh_python_diagnostics(state: &mut Snapshot) {
        let mut cleared = Vec::new();
        for package in &mut state.packages {
            if package.remote
                || !package
                    .issues
                    .iter()
                    .any(|issue| issue.contains("运行环境") && issue.contains(".venv"))
            {
                continue;
            }
            let environments =
                files::local_import_exclusions(Path::new(&package.path)).unwrap_or_default();
            let verified: Vec<_> = environments
                .into_iter()
                .filter(|env| {
                    if !env.file_name().is_some_and(|n| n == ".venv") {
                        return false;
                    }
                    let Some(original) = env.parent() else {
                        return false;
                    };
                    if !files::rebuildable_python_skill(original) {
                        return false;
                    }
                    let Ok(original) = fs::canonicalize(original) else {
                        return false;
                    };
                    state
                        .skills
                        .iter()
                        .filter(|skill| package.member_ids.contains(&skill.id))
                        .any(|skill| {
                            package
                                .scopes
                                .iter()
                                .filter(|scope| {
                                    scope.source_id == skill.source_id
                                        && !scope.origin_path.is_empty()
                                })
                                .any(|scope| {
                                    fs::canonicalize(
                                        Path::new(&scope.origin_path).join(&skill.relative_path),
                                    )
                                    .ok()
                                    .as_ref()
                                        == Some(&original)
                                })
                                && files::rebuildable_python_skill(&skill_path(
                                    Path::new(&state.storage_root),
                                    skill,
                                ))
                        })
                })
                .map(|p| p.display().to_string())
                .collect();
            package.issues.retain(|issue| {
                if issue.contains("运行环境") && verified.iter().any(|path| issue.contains(path))
                {
                    for scope in &package.scopes {
                        cleared.push((scope.source_id.clone(), issue.clone()));
                    }
                    false
                } else {
                    true
                }
            });
        }
        for source in &mut state.sources {
            if !cleared.iter().any(|(sid, _)| sid == &source.id) {
                continue;
            }
            for (_, issue) in cleared.iter().filter(|(sid, _)| sid == &source.id) {
                source.error = source.error.replace(issue, "");
            }
            source.error = source
                .error
                .split('；')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join("；");
            if source.status == "attention" && source.error.is_empty() {
                source.status = "current".into();
            }
        }
    }

    // A filesystem observation never grants permission to remove an installation.
    pub(crate) fn observe_installations(state: &mut Snapshot) {
        state.skill_entity_paths = Some(
            state
                .skills
                .iter()
                .filter_map(|skill| {
                    fs::canonicalize(skill_path(Path::new(&state.storage_root), skill))
                        .ok()
                        .filter(|path| path.join("SKILL.md").is_file())
                        .map(|path| (skill.id.clone(), path.display().to_string()))
                })
                .collect(),
        );
        state.external_installations.clear();
        let mut origins: BTreeMap<PathBuf, Vec<String>> = BTreeMap::new();
        for skill in &state.skills {
            let original = skill.external_path.as_ref().map(PathBuf::from).or_else(|| {
                state
                    .sources
                    .iter()
                    .find(|s| {
                        s.id == skill.source_id
                            && matches!(
                                s.kind.as_str(),
                                "local" | "git" | "local_reference" | "local_managed"
                            )
                    })
                    .map(|s| Path::new(&s.path).join(&skill.relative_path))
            });
            if let Some(path) = original.and_then(|p| fs::canonicalize(p).ok()) {
                if !path.starts_with(&state.storage_root) {
                    origins.entry(path).or_default().push(skill.id.clone());
                }
            }
        }
        for package in &state.packages {
            for scope in &package.scopes {
                if scope.origin_path.is_empty() {
                    continue;
                }
                for skill in state.skills.iter().filter(|skill| {
                    skill.source_id == scope.source_id && package.member_ids.contains(&skill.id)
                }) {
                    if let Ok(actual) =
                        fs::canonicalize(Path::new(&scope.origin_path).join(&skill.relative_path))
                    {
                        if !actual.starts_with(&state.storage_root) {
                            let ids = origins.entry(actual).or_default();
                            if !ids.contains(&skill.id) {
                                ids.push(skill.id.clone());
                            }
                        }
                    }
                }
            }
        }
        for target in &state.targets {
            let Ok(entries) = fs::read_dir(&target.path) else {
                continue;
            };
            for entry in entries.flatten() {
                if !entry.file_type().is_ok_and(|m| m.is_symlink()) {
                    continue;
                }
                let Ok(actual) = fs::canonicalize(entry.path()) else {
                    continue;
                };
                for (origin, ids) in &origins {
                    let Ok(suffix) = origin.strip_prefix(&actual) else {
                        continue;
                    };
                    let installation = if suffix.as_os_str().is_empty() {
                        entry.path()
                    } else {
                        entry.path().join(suffix)
                    };
                    if !installation.join("SKILL.md").is_file()
                        || state
                            .bindings
                            .iter()
                            .any(|b| Path::new(&b.path) == installation)
                    {
                        continue;
                    }
                    for sid in ids {
                        state.external_installations.push(ExternalInstallation {
                            skill_id: sid.clone(),
                            target_id: target.id.clone(),
                            path: installation.display().to_string(),
                            entity_path: origin.display().to_string(),
                        });
                    }
                }
            }
        }
    }

    pub(crate) fn package_migration_preview(&self, request: &Value) -> Result<Value> {
        let preview = self.preview_preset_folder(request)?;
        let items: Vec<_> = preview["items"].as_array().unwrap().iter().map(|item| {
            let original = item["entryPath"].as_str().unwrap_or_default();
            let linked = Path::new(original).ancestors().any(|p| fs::symlink_metadata(p).is_ok_and(|m| m.file_type().is_symlink()));
            json!({"path":original,"entity":item["path"],"skillId":item["existingId"],"status":if linked {"needsSourceRestore"} else {"ready"},"reason":if linked {"原成员含软链；请先审阅备份与当前实体差异，恢复独立来源后再同步。此操作不会还原或改动文件。"} else {"原实体保持不变，可登记为同步包"}})
        }).collect();
        Ok(json!({"revision":preview["revision"],"items":items}))
    }

    pub(crate) fn import_package(&self, request: &Value) -> Result<Value> {
        let preview = self.preview_preset_folder(request)?;
        let root = fs::canonicalize(files::absolute(text(request, "path")?)?)?;
        let baseline = self.snapshot()?;
        if let Some(package) = baseline
            .packages
            .iter()
            .find(|p| Path::new(&p.path) == root)
        {
            return self.sync_registered_package(&baseline, package, request);
        }
        let all = preview["items"].as_array().unwrap();
        if all.is_empty() {
            return fail("未发现可导入的包成员");
        }
        // Snapshot the entire package even when a preset selects only part of it.
        if all.iter().any(|i| {
            i["path"] != i["entryPath"] || i["error"].as_str().is_some_and(|s| !s.is_empty())
        }) {
            return fail("包内成员已是安装软链或位于统一库，请先查看迁移预览并恢复完整来源目录");
        }
        let mut groups: BTreeMap<PathBuf, Vec<String>> = BTreeMap::new();
        for item in all {
            let path = PathBuf::from(text(item, "path")?);
            let repo = git_text(&path, &["rev-parse", "--show-toplevel"]).map(PathBuf::from);
            let group = repo
                .filter(|repo| root.starts_with(repo) || repo.starts_with(&root))
                .unwrap_or(root.clone());
            groups
                .entry(group)
                .or_default()
                .push(path.display().to_string());
        }
        let mut package_id = String::new();
        let mut ids = vec![];
        let mut selected_members = vec![];
        let state = self.transact(
            "import_package",
            "登记并同步 Skill 包",
            |s, library, changes| {
                if request.get("revision").and_then(Value::as_u64) != Some(s.revision as u64) {
                    return fail("资料库已变化，请重新扫描");
                }
                let mut scopes = vec![];
                for (directory, selected) in &groups {
                    let before: BTreeSet<_> = s.sources.iter().map(|s| s.id.clone()).collect();
                    let existing = s
                        .sources
                        .iter()
                        .find(|src| Path::new(&src.path) == directory && src.kind == "local")
                        .cloned();
                    let existing_id = existing.as_ref().map(|s| s.id.clone());
                    self.prepare_import(
                        directory,
                        selected.clone(),
                        false,
                        None,
                        s,
                        library,
                        changes,
                    )?;
                    let source_id = existing_id
                        .or_else(|| {
                            s.sources
                                .iter()
                                .find(|src| {
                                    src.kind == "local" && Path::new(&src.path) == directory
                                })
                                .map(|src| src.id.clone())
                        })
                        .or_else(|| {
                            s.sources
                                .iter()
                                .find(|src| !before.contains(&src.id))
                                .map(|src| src.id.clone())
                        })
                        .ok_or_else(|| error::Error::Message("无法关联包来源".into()))?;
                    let prefix = root
                        .strip_prefix(directory)
                        .unwrap_or(Path::new(""))
                        .to_string_lossy()
                        .replace('\\', "/");
                    let excluded = groups
                        .keys()
                        .filter(|other| *other != directory && other.starts_with(directory))
                        .map(|p| {
                            p.strip_prefix(directory)
                                .unwrap()
                                .to_string_lossy()
                                .replace('\\', "/")
                        })
                        .collect();
                    scopes.push(PackageScope {
                        auto_add: None,
                        origin_path: directory.display().to_string(),
                        source_id: source_id.clone(),
                        prefix,
                        excluded,
                    });
                }
                package_id = s
                    .packages
                    .iter()
                    .find(|p| Path::new(&p.path) == root)
                    .map(|p| p.id.clone())
                    .unwrap_or_else(id);
                s.packages.retain(|p| p.id != package_id);
                let issues = files::python_environment_issues(&root)?;
                s.packages.push(SkillPackage {
                    remote: false,
                    missing_member_ids: vec![],
                    issues,
                    id: package_id.clone(),
                    name: optional(
                        request,
                        "name",
                        root.file_name()
                            .unwrap_or_default()
                            .to_str()
                            .unwrap_or("Skill 包"),
                    )
                    .into(),
                    path: root.display().to_string(),
                    scopes,
                    member_ids: vec![],
                });
                Self::refresh_package_members(s);
                ids = Self::package_selected_ids(s, &package_id, request)?;
                selected_members = s
                    .skills
                    .iter()
                    .filter(|skill| ids.contains(&skill.id))
                    .cloned()
                    .collect::<Vec<_>>();
                Ok(())
            },
        )?;
        let ids = single_content::resolve_import_ids(&selected_members, &state);
        Ok(json!({"snapshot":state,"packageId":package_id,"skillIds":ids}))
    }
    fn sync_registered_package(
        &self,
        baseline: &Snapshot,
        package: &SkillPackage,
        request: &Value,
    ) -> Result<Value> {
        // Read the selected original package, never git-pull a user's checkout.
        // Each scope keeps its own repository boundary and full shared resources.
        let mut prepared = vec![];
        for scope in &package.scopes {
            let origin = fs::canonicalize(&scope.origin_path)?;
            if origin.starts_with(&baseline.storage_root)
                || Path::new(&baseline.storage_root).starts_with(&origin)
            {
                return fail("包来源不能与统一库互相包含");
            }
            let view = self.local_source_view(&origin, true)?;
            let path = view
                .as_ref()
                .map(|v| v.path())
                .unwrap_or(&origin)
                .to_path_buf();
            let digest = files::content_digest(&path)?;
            let scanned = files::scan(&path)?;
            let entries = scanned
                .items
                .into_iter()
                .filter(|i| i.status == "ready")
                .filter_map(|i| {
                    let relative = Path::new(&i.path)
                        .strip_prefix(&path)
                        .ok()?
                        .to_string_lossy()
                        .replace('\\', "/");
                    (within(&relative, &scope.prefix)
                        && !scope.excluded.iter().any(|p| within(&relative, p)))
                    .then_some((relative, i))
                })
                .collect::<BTreeMap<_, _>>();
            prepared.push((scope.clone(), origin, view, path, digest, entries));
        }
        let mut ids = vec![];
        let mut selected_members = vec![];
        self.transact("sync_package", "同步原包成员变化", |s, root, _| {
            if request.get("revision").and_then(Value::as_u64) != Some(s.revision as u64) {
                return fail("资料库已变化，请重新扫描");
            }
            let mut missing = vec![];
            let mut issues = vec![];
            for (scope, origin, _, path, digest, entries) in &prepared {
                if files::snapshot_current_content(root, path)? != *digest {
                    return fail("来源在同步期间变化，请重新扫描");
                }
                issues.extend(files::python_environment_issues(origin)?);
                for member in s.skills.iter_mut().filter(|m| {
                    m.source_id == scope.source_id
                        && within(&m.relative_path, &scope.prefix)
                        && !scope.excluded.iter().any(|p| within(&m.relative_path, p))
                }) {
                    if let Some(item) = entries.get(&member.relative_path) {
                        member.bundle_digest = digest.clone();
                        member.version = digest[..12].to_string();
                        member.name = item.name.clone();
                        member.description = item.description.clone();
                    } else {
                        missing.push(member.id.clone());
                    }
                }
                for (relative, item) in entries {
                    if !s
                        .skills
                        .iter()
                        .any(|m| m.source_id == scope.source_id && m.relative_path == *relative)
                    {
                        s.skills.push(Skill {
                            id: id(),
                            name: item.name.clone(),
                            description: item.description.clone(),
                            source_id: scope.source_id.clone(),
                            bundle_digest: digest.clone(),
                            relative_path: relative.clone(),
                            version: digest[..12].to_string(),
                            installed_at: now(),
                            external_path: None,
                        });
                    }
                }
                if let Some(source) = s.sources.iter_mut().find(|src| src.id == scope.source_id) {
                    source.version = digest.clone();
                    source.last_checked = now();
                    source.status = if missing.is_empty() && issues.is_empty() {
                        "current"
                    } else {
                        "attention"
                    }
                    .into();
                    source.error = if missing.is_empty() {
                        issues.join("；")
                    } else {
                        "部分成员已从来源移除，保留旧安装，请在预设中确认处理".into()
                    };
                }
            }
            let p = s
                .packages
                .iter_mut()
                .find(|p| p.id == package.id)
                .ok_or_else(|| error::Error::Message("包已移除".into()))?;
            p.missing_member_ids = missing;
            p.issues = issues;
            for scope in &package.scopes {
                Self::update_package_presets(s, &scope.source_id);
            }
            ids = Self::package_selected_ids(s, &package.id, request)?;
            selected_members = s
                .skills
                .iter()
                .filter(|skill| ids.contains(&skill.id))
                .cloned()
                .collect::<Vec<_>>();
            Ok(())
        })?;
        self.reconcile_packages()?;
        let state = self.snapshot()?;
        let ids = single_content::resolve_import_ids(&selected_members, &state);
        Ok(json!({"snapshot":state,"packageId":package.id,"skillIds":ids}))
    }
    fn package_selected_ids(
        state: &Snapshot,
        package_id: &str,
        request: &Value,
    ) -> Result<Vec<String>> {
        let package = state.packages.iter().find(|p| p.id == package_id).unwrap();
        let selected = request
            .get("selectedPaths")
            .map(|_| strings(request, "selectedPaths"))
            .transpose()?
            .unwrap_or_default();
        let ids: Vec<_> = package
            .member_ids
            .iter()
            .filter(|id| !package.missing_member_ids.contains(id))
            .filter(|id| {
                (request.get("selectedPaths").is_none() && !package.missing_member_ids.contains(id))
                    || state
                        .skills
                        .iter()
                        .find(|s| &s.id == *id)
                        .is_some_and(|skill| {
                            package
                                .scopes
                                .iter()
                                .filter(|scope| scope.source_id == skill.source_id)
                                .any(|scope| {
                                    let root = if scope.origin_path.is_empty() {
                                        state
                                            .sources
                                            .iter()
                                            .find(|s| s.id == skill.source_id)
                                            .map(|s| s.path.as_str())
                                            .unwrap_or("")
                                    } else {
                                        &scope.origin_path
                                    };
                                    selected.contains(
                                        &Path::new(root)
                                            .join(&skill.relative_path)
                                            .display()
                                            .to_string(),
                                    )
                                })
                        })
            })
            .cloned()
            .collect();
        if !selected.is_empty() && ids.len() != selected.iter().collect::<BTreeSet<_>>().len() {
            return fail("部分所选成员无法关联到该包，请重新扫描");
        }
        Ok(ids)
    }
    pub(crate) fn package_accepts_new(state: &Snapshot, source_id: &str, relative: &str) -> bool {
        state.packages.iter().any(|package| {
            package.scopes.iter().any(|scope| {
                scope.source_id == source_id
                    && within(relative, &scope.prefix)
                    && !scope.excluded.iter().any(|path| within(relative, path))
                    && (scope.auto_add != Some(false)
                        || state.preset_packages.iter().any(|subscription| {
                            subscription.package_id == package.id && subscription.auto_add
                        }))
            })
        })
    }
    pub(crate) fn refresh_package_members(state: &mut Snapshot) {
        for package in &mut state.packages {
            package.member_ids = state
                .skills
                .iter()
                .filter(|skill| {
                    package.scopes.iter().any(|scope| {
                        scope.source_id == skill.source_id
                            && within(&skill.relative_path, &scope.prefix)
                            && !scope
                                .excluded
                                .iter()
                                .any(|p| within(&skill.relative_path, p))
                    })
                })
                .map(|s| s.id.clone())
                .collect();
        }
    }
    pub(crate) fn update_package_presets(state: &mut Snapshot, source_id: &str) {
        Self::refresh_package_members(state);
        let affected: BTreeSet<_> = state
            .packages
            .iter()
            .filter(|p| p.scopes.iter().any(|s| s.source_id == source_id))
            .map(|p| p.id.clone())
            .collect();
        for preset in &mut state.presets {
            let before = serde_json::to_value(&*preset).unwrap();
            for subscription in state
                .preset_packages
                .iter_mut()
                .filter(|p| p.preset_id == preset.id && affected.contains(&p.package_id))
            {
                let Some(package) = state
                    .packages
                    .iter()
                    .find(|p| p.id == subscription.package_id)
                else {
                    continue;
                };
                let selected = if subscription.auto_add {
                    package
                        .member_ids
                        .iter()
                        .filter(|id| {
                            !subscription.excluded_ids.contains(id)
                                && (!package.missing_member_ids.contains(id)
                                    || subscription.selected_ids.contains(id))
                        })
                        .cloned()
                        .collect::<Vec<_>>()
                } else {
                    subscription.selected_ids.clone()
                };
                for sid in &selected {
                    if !preset.skill_ids.contains(sid) {
                        preset.skill_ids.push(sid.clone());
                    }
                    if let Some(skill) = state.skills.iter().find(|s| &s.id == sid) {
                        preset.locks.insert(sid.clone(), skill.clone());
                    }
                }
                subscription.selected_ids = selected;
            }
            if before != serde_json::to_value(&*preset).unwrap() {
                preset.revision += 1;
            }
        }
    }
    pub(crate) fn reconcile_packages(&self) -> Result<()> {
        let state = self.snapshot()?;
        let targets: BTreeSet<_> = state
            .preset_applications
            .iter()
            .filter(|a| {
                a.follow
                    && (!a.error.is_empty()
                        || state
                            .presets
                            .iter()
                            .any(|p| p.id == a.preset_id && p.revision != a.applied_revision))
            })
            .map(|a| a.target_id.clone())
            .collect();
        for target_id in targets {
            let result = self.transact(
                "preset_sync",
                "同步目标的预设版本",
                |s, root, changes| {
                    let applications: Vec<_> = s
                        .preset_applications
                        .iter()
                        .filter(|a| a.follow && a.target_id == target_id)
                        .cloned()
                        .collect();
                    for application in &applications {
                        let preset = s
                            .presets
                            .iter()
                            .find(|p| p.id == application.preset_id)
                            .cloned()
                            .ok_or_else(|| error::Error::Message("预设不存在".into()))?;
                        let claim = format!("preset:{}", preset.id);
                        let old: Vec<_> = s
                            .bindings
                            .iter()
                            .filter(|b| {
                                b.target_id == target_id
                                    && b.claims.contains(&claim)
                                    && !preset.skill_ids.contains(&b.skill_id)
                            })
                            .map(|b| b.id.clone())
                            .collect();
                        crate::operations::revoke(s, root, changes, &old, &claim)?;
                        let skills = preset
                            .skill_ids
                            .iter()
                            .map(|id| {
                                preset
                                    .locks
                                    .get(id)
                                    .or_else(|| s.skills.iter().find(|skill| &skill.id == id))
                                    .cloned()
                                    .ok_or_else(|| {
                                        error::Error::Message("预设锁定成员不存在".into())
                                    })
                            })
                            .collect::<Result<Vec<_>>>()?;
                        if !skills.is_empty() {
                            crate::operations::distribute(
                                s,
                                root,
                                changes,
                                &skills,
                                &[target_id.clone()],
                                &claim,
                                false,
                                false,
                                &[],
                            )?;
                        }
                        let a = s
                            .preset_applications
                            .iter_mut()
                            .find(|a| a.preset_id == preset.id && a.target_id == target_id)
                            .unwrap();
                        a.applied_revision = preset.revision;
                        a.error.clear();
                    }
                    Ok(())
                },
            );
            if let Err(e) = result {
                self.transact(
                    "preset_sync_failed",
                    "预设目标同步待重试",
                    |s, _, _| {
                        for a in s
                            .preset_applications
                            .iter_mut()
                            .filter(|a| a.follow && a.target_id == target_id)
                        {
                            a.error = e.to_string();
                        }
                        Ok(())
                    },
                )?;
            }
        }
        Ok(())
    }
}
