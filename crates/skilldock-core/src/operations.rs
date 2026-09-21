use crate::{
    error::{Result, fail},
    files::{self, Change},
    *,
};
use std::collections::{BTreeMap, BTreeSet};

// Re-adding a target restores only links whose exact snapshot and identity are known.
// Names alone cannot identify a Skill, and external links retain their existing ownership.
fn restore_target_bindings(
    s: &mut Snapshot,
    root: &Path,
    target: &Target,
    changes: &mut Vec<Change>,
) -> Result<()> {
    let mut candidates = s.skills.clone();
    candidates.extend(s.presets.iter().flat_map(|p| p.locks.values()).cloned());
    for binding in &s.bindings {
        if let Some(skill) = s.skills.iter().find(|skill| skill.id == binding.skill_id) {
            let mut version = skill.clone();
            version.bundle_digest = binding.digest.clone();
            version.relative_path = binding.relative_path.clone();
            version.version = binding.version.clone();
            version.external_path = binding.external_path.clone();
            candidates.push(version);
        }
    }
    let mut verified = BTreeMap::new();
    for entry in fs::read_dir(&target.path)? {
        let entry = entry?;
        let path = entry.path();
        if !entry.file_type()?.is_symlink() || s.bindings.iter().any(|b| Path::new(&b.path) == path)
        {
            continue;
        }
        let Ok(actual) = fs::canonicalize(&path) else {
            continue;
        };
        if !actual.join("SKILL.md").is_file() {
            continue;
        }
        let mut matches = candidates.iter().filter(|skill| {
            skill.external_path.is_none()
                && !skill.bundle_digest.is_empty()
                && s.skills.iter().any(|current| current.id == skill.id)
                && skill_path(root, skill) == actual
        });
        let Some(skill) = matches.next() else {
            continue;
        };
        if matches.any(|other| other.id != skill.id || other.version != skill.version) {
            continue;
        }
        let valid = *verified
            .entry(skill.bundle_digest.clone())
            .or_insert_with(|| {
                files::snapshot_matches(
                    &root.join("objects").join(&skill.bundle_digest).join("tree"),
                    &skill.bundle_digest,
                )
                .unwrap_or(false)
            });
        if !valid {
            continue;
        }
        let before = fs::read_link(&path)?;
        if before != actual {
            changes.push(Change {
                restore: false,
                backup_digest: None,
                path: path.clone(),
                before: Some(before),
                after: Some(actual),
                backup: None,
            });
        }
        s.bindings.push(Binding {
            id: id(),
            skill_id: skill.id.clone(),
            target_id: target.id.clone(),
            path: path.display().to_string(),
            version: skill.version.clone(),
            digest: skill.bundle_digest.clone(),
            relative_path: skill.relative_path.clone(),
            claims: vec!["manual".into()],
            follow: false,
            borrowed: false,
            original_link: None,
            external_path: None,
        });
    }
    Ok(())
}

pub(crate) fn inferred_target(path: &Path, profiles: &[AgentProfile]) -> Target {
    for profile in profiles {
        for configured in &profile.user_paths {
            if let Ok(configured) = files::absolute(configured) {
                if path == configured || fs::canonicalize(&configured).ok().as_deref() == Some(path)
                {
                    return Target {
                        id: id(),
                        name: format!("{} · 用户级", profile.name),
                        tool: profile.id.clone(),
                        scope: "user".into(),
                        path: path.display().to_string(),
                    };
                }
            }
        }
    }
    for profile in profiles {
        for relative in &profile.project_paths {
            let relative = Path::new(relative);
            if relative.components().count() > 1 && path.ends_with(relative) {
                let mut project = path;
                for _ in relative.components() {
                    project = project.parent().unwrap_or(project);
                }
                let name = project.file_name().unwrap_or_default().to_string_lossy();
                return Target {
                    id: id(),
                    name: format!("{} · {}", profile.name, name),
                    tool: profile.id.clone(),
                    scope: "project".into(),
                    path: path.display().to_string(),
                };
            }
        }
    }
    let leaf = path.file_name().unwrap_or_default().to_string_lossy();
    let name = if leaf.eq_ignore_ascii_case("skills") {
        format!(
            "{} · Skills",
            path.parent()
                .and_then(Path::file_name)
                .unwrap_or_default()
                .to_string_lossy()
        )
    } else {
        leaf.to_string()
    };
    Target {
        id: id(),
        name,
        tool: "custom".into(),
        scope: "user".into(),
        path: path.display().to_string(),
    }
}

fn parse_agent_profiles(value: &Value) -> Result<Vec<AgentProfile>> {
    let mut profiles: Vec<AgentProfile> = serde_json::from_value(value.clone())?;
    profiles.retain(|profile| !profile.id.eq_ignore_ascii_case("qclaw"));
    if profiles.len() > 64 {
        return fail("最多配置 64 个工具");
    }
    let mut ids = BTreeSet::new();
    for profile in &mut profiles {
        if profile.id.trim().is_empty()
            || !ids.insert(profile.id.clone())
            || profile.name.trim().is_empty()
        {
            return fail("工具名称和标识不能为空，标识不能重复");
        }
        for (paths, project) in [
            (&mut profile.user_paths, false),
            (&mut profile.project_paths, true),
        ] {
            if paths.len() > 32 {
                return fail("每种作用域最多配置 32 个路径");
            }
            let mut seen = BTreeSet::new();
            for path in paths {
                *path = path
                    .trim()
                    .replace('\\', "/")
                    .trim_end_matches('/')
                    .to_string();
                if path.is_empty() {
                    return fail("Skill 路径不能为空");
                }
                if project {
                    if path.starts_with('~') || path.contains(':') {
                        return fail("项目路径应填写相对于项目的子目录");
                    }
                    let relative = files::safe_relative(path)?;
                    if relative.as_os_str().is_empty() || path == "." {
                        return fail("请指定项目内的 Skill 子目录");
                    }
                } else {
                    if !path.starts_with("~/") && !Path::new(path).is_absolute() {
                        return fail("用户目录需要绝对路径或 ~/ 路径");
                    }
                    let absolute = files::absolute(path)?;
                    if absolute == files::home()? || files::protected(&absolute) {
                        return fail("请指定专用 Skill 目录");
                    }
                }
                if !seen.insert(path.clone()) {
                    return fail("同一工具的路径不能重复");
                }
            }
        }
    }
    Ok(profiles)
}

fn parse_catalog_sites(value: &Value) -> Result<Vec<CatalogSite>> {
    let mut sites: Vec<CatalogSite> = serde_json::from_value(value.clone())?;
    if sites.is_empty() || sites.len() > 8 {
        return fail("请配置 1 至 8 个搜索站点");
    }
    let mut urls = BTreeSet::new();
    for site in &mut sites {
        site.name = site.name.trim().to_owned();
        if site.name.is_empty() || site.name.chars().count() > 100 {
            return fail("网站名称不能为空，且不能超过 100 个字符");
        }
        let url = reqwest::Url::parse(site.url.trim())
            .map_err(|_| error::Error::Message("网站地址无效".into()))?;
        if url.scheme() != "https"
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return fail("站点需要不含凭据、查询参数或片段的 HTTPS 地址");
        }
        site.url = url.as_str().trim_end_matches('/').to_owned();
        if !urls.insert(CatalogSite::legacy(&site.url).url) {
            return fail("网站地址不能重复");
        }
    }
    Ok(sites)
}

fn unique(values: Vec<String>) -> Vec<String> {
    values
        .into_iter()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}
fn lookup_skill<'a>(s: &'a Snapshot, id: &str) -> Result<&'a Skill> {
    s.skills
        .iter()
        .find(|x| x.id == id)
        .ok_or_else(|| error::Error::Message("Skill 不存在".into()))
}
fn expected(s: &Snapshot, r: &Value) -> Result<()> {
    if r.get("expectedRevision").and_then(Value::as_u64) != Some(s.revision as u64) {
        return fail("数据已变化，请重新预览分发计划");
    }
    Ok(())
}
fn build_plan(
    s: &Snapshot,
    skills: &[Skill],
    targets: &[String],
    claim: &str,
    pending: &[Change],
    adopt_existing: bool,
    replace_bindings: &[String],
) -> Result<DistributionPlan> {
    if skills.is_empty() || targets.is_empty() {
        return fail("请选择 Skill 和目标");
    }
    let root = Path::new(&s.storage_root);
    let mut items = vec![];
    let mut paths = BTreeSet::new();
    let mut comparisons = BTreeMap::new();
    for target_id in targets {
        let target = s
            .targets
            .iter()
            .find(|t| &t.id == target_id)
            .ok_or_else(|| error::Error::Message("分发目标不存在".into()))?;
        if !Path::new(&target.path).is_dir() {
            return fail("目标目录不可用");
        }
        for skill in skills {
            let leaf = skill
                .external_path
                .as_ref()
                .and_then(|path| Path::new(path).file_name())
                .and_then(|name| name.to_str())
                .unwrap_or(&skill.name);
            let same_name_bindings: Vec<_> = s
                .bindings
                .iter()
                .filter(|b| {
                    b.target_id == target.id
                        && s.skills.iter().any(|old| {
                            old.id == b.skill_id && old.name.eq_ignore_ascii_case(&skill.name)
                        })
                })
                .collect();
            let path = s
                .bindings
                .iter()
                .find(|b| b.target_id == target.id && b.skill_id == skill.id)
                .map(|b| PathBuf::from(&b.path))
                .or_else(|| {
                    s.external_installations
                        .iter()
                        .find(|i| i.target_id == target.id && i.skill_id == skill.id)
                        .map(|i| PathBuf::from(&i.path))
                })
                .or_else(|| {
                    (same_name_bindings.len() == 1)
                        .then(|| PathBuf::from(&same_name_bindings[0].path))
                })
                .unwrap_or(Path::new(&target.path).join(files::clean_name(leaf)?));
            let mut item = PlanItem {
                replacement: None,
                skill_id: skill.id.clone(),
                target_id: target.id.clone(),
                path: path.display().to_string(),
                action: "create".into(),
                error: String::new(),
            };
            let same_local_link = skill.external_path.is_some()
                && fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_symlink())
                && fs::canonicalize(&path).ok().is_some_and(|entity| {
                    Some(entity) == fs::canonicalize(skill_path(root, skill)).ok()
                });
            if same_name_bindings.len() > 1
                && !same_name_bindings.iter().any(|b| b.skill_id == skill.id)
            {
                item.error = "目标中存在多个同名安装，请先在分发目标中处理重复关系".into();
            }
            if path
                .parent()
                .and_then(|p| fs::canonicalize(p).ok())
                .as_deref()
                != path.parent()
            {
                item.error = "安装入口位于父目录软链内部，请先单独处理整包入口".into();
            }
            if !paths.insert(item.path.to_lowercase()) {
                item.error = "所选成员存在同名路径，请分开处理".into();
            }
            if let Some(package) = s
                .packages
                .iter()
                .find(|p| p.member_ids.contains(&skill.id) && !p.issues.is_empty())
            {
                item.error = package.issues.join("；");
            }
            if !files::has_skill_entry(&skill_path(root, skill)) {
                item.error = "中央库内容缺失".into();
            }
            if let Some(b) = s.bindings.iter().find(|b| b.path == item.path) {
                item.action = if b.borrowed && same_local_link {
                    if adopt_existing { "adopt" } else { "borrow" }
                } else {
                    "reuse"
                }
                .into();
                if !binding_matches(root, b)
                    && !pending.iter().any(|c| {
                        c.path == Path::new(&b.path)
                            && c.after.as_ref() == Some(&binding_path(root, b))
                    })
                {
                    item.error = "已有链接已被外部修改或丢失".into();
                }
                let same_entity = claim == "manual"
                    && fs::canonicalize(binding_path(root, b))
                        .ok()
                        .is_some_and(|path| {
                            Some(path) == fs::canonicalize(skill_path(root, skill)).ok()
                        });
                if b.skill_id != skill.id && same_entity {
                    // Adding a manual claim to the same entity is not a source switch.
                    // Keep the existing identity, ownership and restoration metadata.
                    item.action = "reuse".into();
                } else if b.skill_id != skill.id {
                    let blocking_claims: Vec<_> = b
                        .claims
                        .iter()
                        .filter(|c| c.as_str() != "manual" && c.as_str() != claim)
                        .cloned()
                        .collect();
                    let previous = binding_path(root, b);
                    let next = skill_path(root, skill);
                    let key = (previous.clone(), next.clone());
                    let content_equal = *comparisons.entry(key).or_insert_with(|| {
                        files::content_digest(&previous)
                            .ok()
                            .zip(files::content_digest(&next).ok())
                            .map(|(a, b)| a == b)
                    });
                    item.replacement = Some(PlanReplacement {
                        next_entity_path: next.display().to_string(),
                        next_version: skill.version.clone(),
                        binding_id: b.id.clone(),
                        skill_id: b.skill_id.clone(),
                        entity_path: previous.display().to_string(),
                        version: b.version.clone(),
                        claims: b.claims.clone(),
                        blocking_claims: blocking_claims.clone(),
                        content_equal,
                        restores_original: b.borrowed || b.original_link.is_some(),
                    });
                    if item.error.is_empty() {
                        if !blocking_claims.is_empty() {
                            item.error =
                                "旧来源仍被其他预设或本地来源引用，请先解除这些引用".into();
                        } else if !fs::symlink_metadata(&path)
                            .is_ok_and(|m| m.file_type().is_symlink())
                        {
                            item.error = "目标不是可管理的软链，拒绝切换来源".into();
                        } else if replace_bindings.contains(&b.id) {
                            item.action = "replace".into();
                        } else {
                            item.error = "目标已使用其他来源，请确认切换来源".into();
                        }
                    }
                } else if b.digest != skill.bundle_digest
                    || b.relative_path != skill.relative_path
                    || b.external_path != skill.external_path
                {
                    item.action = "update".into();
                    if b.claims.iter().any(|c| {
                        c != claim
                            && !c.strip_prefix("preset:").is_some_and(|pid| {
                                s.preset_applications.iter().any(|a| {
                                    a.preset_id == pid && a.target_id == b.target_id && a.follow
                                }) && s
                                    .presets
                                    .iter()
                                    .find(|p| p.id == pid)
                                    .and_then(|p| p.locks.get(&skill.id))
                                    .is_some_and(|other| {
                                        other.bundle_digest == skill.bundle_digest
                                            && other.relative_path == skill.relative_path
                                            && other.external_path == skill.external_path
                                    })
                            })
                    }) {
                        item.error = "其他预设或手动分发仍锁定旧版本".into();
                    }
                }
            } else if files::exists(&path) {
                if same_local_link {
                    item.action = if adopt_existing { "adopt" } else { "borrow" }.into();
                } else if s.external_installations.iter().any(|i| {
                    i.skill_id == skill.id
                        && i.target_id == target.id
                        && Path::new(&i.path) == path
                        && fs::canonicalize(&path).ok() == Some(PathBuf::from(&i.entity_path))
                }) && fs::symlink_metadata(&path)?.file_type().is_symlink()
                {
                    item.action = "takeover".into();
                } else {
                    item.error = "目标存在非本工具管理的内容，不覆盖".into();
                }
            }
            if !item.error.is_empty() {
                item.action = "conflict".into();
            }
            items.push(item);
        }
    }
    Ok(DistributionPlan {
        items,
        revision: s.revision,
    })
}
pub(crate) fn distribute(
    s: &mut Snapshot,
    root: &Path,
    changes: &mut Vec<Change>,
    skills: &[Skill],
    targets: &[String],
    claim: &str,
    allow_takeover: bool,
    adopt_existing: bool,
    replace_bindings: &[String],
) -> Result<()> {
    let plan = build_plan(
        s,
        skills,
        targets,
        claim,
        changes,
        adopt_existing,
        replace_bindings,
    )?;
    if replace_bindings.iter().any(|id| {
        !plan
            .items
            .iter()
            .any(|i| i.replacement.as_ref().is_some_and(|r| &r.binding_id == id))
    }) {
        return fail("来源切换选择已失效，请重新预览");
    }
    if !allow_takeover && plan.items.iter().any(|i| i.action == "takeover") {
        return fail("存在外部已有安装，请确认接管后再应用预设");
    }
    if plan.items.iter().any(|i| !i.error.is_empty()) {
        return fail(
            plan.items
                .iter()
                .filter(|i| !i.error.is_empty())
                .map(|i| format!("{}：{}", i.path, i.error))
                .collect::<Vec<_>>()
                .join("；"),
        );
    }
    let digests: BTreeSet<_> = skills
        .iter()
        .filter(|x| x.external_path.is_none())
        .map(|x| x.bundle_digest.clone())
        .collect();
    for digest in digests {
        if !files::snapshot_matches(&root.join("objects").join(&digest).join("tree"), &digest)? {
            return fail(format!(
                "统一库快照内容已变化，请重新收录当前内容后再分发（现有链接保留；快照 {digest}）"
            ));
        }
    }
    for item in plan.items {
        let skill = skills.iter().find(|x| x.id == item.skill_id).unwrap();
        if let Some(b) = s.bindings.iter_mut().find(|b| b.path == item.path) {
            if item.action == "replace" {
                let before = fs::read_link(&b.path)?;
                if !binding_matches(root, b) {
                    return fail("来源切换前目标链接已变化，请重新预览");
                }
                changes.push(Change {
                    restore: false,
                    backup_digest: None,
                    path: PathBuf::from(&b.path),
                    before: Some(before.clone()),
                    after: Some(skill_path(root, skill)),
                    backup: None,
                });
                if b.borrowed {
                    b.original_link = Some(before.display().to_string());
                }
                b.borrowed = false;
                b.skill_id = skill.id.clone();
                b.external_path = skill.external_path.clone();
                b.digest = skill.bundle_digest.clone();
                b.version = skill.version.clone();
                b.relative_path = skill.relative_path.clone();
                b.follow = false;
            }
            if item.action == "adopt" {
                // Normalize relative links so managed-link validation can verify them exactly.
                changes.push(Change {
                    restore: false,
                    backup_digest: None,
                    path: PathBuf::from(&b.path),
                    before: Some(fs::read_link(&b.path)?),
                    after: Some(skill_path(root, skill)),
                    backup: None,
                });
                b.borrowed = false;
                b.original_link = None;
            }
            if item.action == "update" {
                changes.push(Change {
                    restore: false,
                    backup_digest: None,
                    path: PathBuf::from(&b.path),
                    before: Some(binding_path(root, b)),
                    after: Some(skill_path(root, skill)),
                    backup: None,
                });
                b.external_path = skill.external_path.clone();
                b.digest = skill.bundle_digest.clone();
                b.version = skill.version.clone();
                b.relative_path = skill.relative_path.clone();
            }
            if !b.claims.iter().any(|c| c == claim) {
                b.claims.push(claim.into());
            }
        } else {
            if item.action != "borrow" {
                changes.push(Change {
                    restore: false,
                    backup_digest: None,
                    path: PathBuf::from(&item.path),
                    before: if matches!(item.action.as_str(), "takeover" | "adopt") {
                        Some(fs::read_link(&item.path)?)
                    } else {
                        None
                    },
                    after: Some(skill_path(root, skill)),
                    backup: None,
                });
            }
            s.bindings.push(Binding {
                original_link: if item.action == "takeover" {
                    Some(fs::read_link(&item.path)?.display().to_string())
                } else {
                    None
                },
                external_path: skill.external_path.clone(),
                borrowed: item.action == "borrow",
                id: id(),
                skill_id: skill.id.clone(),
                target_id: item.target_id,
                path: item.path,
                version: skill.version.clone(),
                digest: skill.bundle_digest.clone(),
                relative_path: skill.relative_path.clone(),
                claims: vec![claim.into()],
                follow: false,
            });
        }
    }
    Ok(())
}
pub(crate) fn revoke(
    s: &mut Snapshot,
    root: &Path,
    changes: &mut Vec<Change>,
    binding_ids: &[String],
    claim: &str,
) -> Result<()> {
    for bid in binding_ids {
        let b = s
            .bindings
            .iter_mut()
            .find(|b| &b.id == bid)
            .ok_or_else(|| error::Error::Message("分发记录不存在".into()))?;
        if !b.claims.iter().any(|c| c == claim) {
            continue;
        }
        b.claims.retain(|c| c != claim);
        if b.claims.is_empty() && !b.borrowed {
            let path = PathBuf::from(&b.path);
            if files::exists(&path) {
                if !binding_matches(root, b) {
                    return fail("目标已被其他内容替换，拒绝取消");
                }
                if let Some(original) = &b.original_link {
                    let dest = PathBuf::from(original);
                    let actual = if dest.is_absolute() {
                        dest
                    } else {
                        path.parent().unwrap().join(dest)
                    };
                    if !actual.join("SKILL.md").is_file() {
                        return fail("原外部安装实体已失效，保留当前链接，请先处理恢复位置");
                    }
                }
                changes.push(Change {
                    restore: false,
                    backup_digest: None,
                    path,
                    before: Some(binding_path(root, b)),
                    after: b.original_link.as_ref().map(PathBuf::from),
                    backup: None,
                });
            }
        }
    }
    s.bindings.retain(|b| !b.claims.is_empty());
    Ok(())
}

// Keep the source basename: SKILL.md may legitimately omit its name field.
pub(crate) struct SourceView {
    _temp: tempfile::TempDir,
    directory: PathBuf,
}
impl SourceView {
    pub(crate) fn path(&self) -> &Path {
        &self.directory
    }
}

impl Engine {
    // Existing agent roots often mix real directories and installations linked by other managers.
    // Exclude those top-level installation links without dereferencing or changing them.
    pub(crate) fn local_source_view(
        &self,
        path: &Path,
        import_only: bool,
    ) -> Result<Option<SourceView>> {
        let single_skill = path.join("SKILL.md").is_file();
        let known = self
            .discover_configured()?
            .iter()
            .chain(self.snapshot()?.targets.iter())
            .any(|t| fs::canonicalize(&t.path).ok().as_deref() == Some(path));
        // Portable imports do not include repository metadata or local Python
        // runtimes. Adoption retains full content for backup/restore integrity.
        let mut excluded = if import_only {
            files::local_import_exclusions(path)?
        } else {
            vec![]
        };
        for entry in fs::read_dir(path)? {
            let e = entry?;
            if !single_skill
                && (e
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".skilldock-backup-")
                    || (known
                        && e.file_type()?.is_symlink()
                        && (!e.path().exists() || e.path().join("SKILL.md").is_file())))
            {
                excluded.push(e.path());
            }
        }
        if excluded.is_empty() {
            return Ok(None);
        }
        let temp = tempfile::tempdir()?;
        let directory = temp.path().join(path.file_name().unwrap_or_default());
        files::copy_tree_excluding(path, &directory, &excluded)?;
        Ok(Some(SourceView {
            _temp: temp,
            directory,
        }))
    }
    pub(crate) fn remote_source_view(path: &Path) -> Result<Option<SourceView>> {
        let exclusions = files::local_import_exclusions(path)?;
        if exclusions.is_empty() {
            return Ok(None);
        }
        let temp = tempfile::tempdir()?;
        let directory = temp.path().join(path.file_name().unwrap_or_default());
        files::copy_tree_excluding(path, &directory, &exclusions)?;
        Ok(Some(SourceView {
            _temp: temp,
            directory,
        }))
    }

    pub(crate) fn import_folder(
        &self,
        path: &Path,
        selected: Vec<String>,
        adopt: bool,
        source_info: Option<Source>,
    ) -> Result<Snapshot> {
        self.transact(
            "import",
            "导入 Skill 到统一目录",
            |state, root, changes| {
                self.prepare_import(path, selected, adopt, source_info, state, root, changes)
            },
        )
    }

    pub(crate) fn prepare_import(
        &self,
        path: &Path,
        selected: Vec<String>,
        adopt: bool,
        source_info: Option<Source>,
        s: &mut Snapshot,
        root: &Path,
        changes: &mut Vec<Change>,
    ) -> Result<()> {
        let path = files::absolute(&path.display().to_string())?;
        if files::protected(&path) || fs::symlink_metadata(&path)?.file_type().is_symlink() {
            return fail("该目录由其他工具管理或是软链，不能接管为来源");
        }
        let path = fs::canonicalize(path)?;
        if adopt
            && (s
                .packages
                .iter()
                .any(|p| path.starts_with(&p.path) || Path::new(&p.path).starts_with(&path))
                || s.skills
                    .iter()
                    .filter_map(|s| s.external_path.as_ref())
                    .any(|p| path.starts_with(p) || Path::new(p).starts_with(&path)))
        {
            return fail("该目录属于已登记的包或本地引用，请使用同步更新，不能移动式归集");
        }
        let local_view = if source_info.is_none() {
            self.local_source_view(&path, !adopt)?
        } else {
            Self::remote_source_view(&path)?
        };
        let snapshot_source = local_view.as_ref().map(|v| v.path()).unwrap_or(&path);
        // Members and selected paths must refer to the content actually snapshotted.
        let mut scanned = files::scan(snapshot_source)?;
        for item in &mut scanned.items {
            let relative = Path::new(&item.path)
                .strip_prefix(snapshot_source)
                .map_err(|_| error::Error::Message("扫描成员不在来源目录中".into()))?;
            item.path = if relative.as_os_str().is_empty() {
                path.display().to_string()
            } else {
                path.join(relative).display().to_string()
            };
        }
        let mut chosen: Vec<_> = scanned
            .items
            .into_iter()
            .filter(|i| i.status == "ready" && (selected.is_empty() || selected.contains(&i.path)))
            .collect();
        if chosen.is_empty() {
            return fail(
                "未发现可导入的 Skill；所选目录可能属于 Git 元数据或本地 Python 环境/缓存",
            );
        }
        chosen.sort_by_key(|i| i.path.len());
        // Nested skills remain independent library entries. Only the outermost
        // selected directory owns a physical link and its corresponding binding.
        let adoption_roots: Vec<_> = chosen
            .iter()
            .filter(|item| {
                !chosen.iter().any(|parent| {
                    parent.path != item.path && Path::new(&item.path).starts_with(&parent.path)
                })
            })
            .map(|item| item.path.clone())
            .collect();
        if root.starts_with(&path)
            || (path.starts_with(root)
                && !(source_info.is_some() && path.starts_with(root.join("cache"))))
        {
            return fail("来源目录不能与中央库互相包含");
        }
        let digest = files::snapshot_current_content(root, snapshot_source)?;
        let mut source = source_info.unwrap_or(Source {
            updates_removed: None,
            local_member_ids: None,
            id: id(),
            name: path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into(),
            kind: "local".into(),
            path: path.display().to_string(),
            url: String::new(),
            reference: String::new(),
            scan_subdir: String::new(),
            version: digest.clone(),
            policy: Policy::default(),
            last_checked: String::new(),
            next_check: String::new(),
            status: if adopt { "detached" } else { "current" }.into(),
            error: if adopt {
                "原安装位置已归集；此记录仅保留归集来源，不代表共同的更新仓库"
            } else {
                ""
            }
            .into(),
        });
        if adopt {
            source.policy.mode = "off".into();
            source.next_check.clear();
        }
        if !adopt {
            source.policy.mode = s.settings.update_mode.clone();
        }
        if let Some(old) = s.sources.iter().find(|x| {
            x.kind == source.kind
                && x.url == source.url
                && x.reference == source.reference
                && x.scan_subdir == source.scan_subdir
                && (source.kind != "local" || x.path == source.path)
        }) {
            source.id = old.id.clone();
            if !adopt {
                source.policy = old.policy.clone();
            }
        }
        // Legacy snapshots included Finder preferences in their identity. Only
        // normalize an existing source when every member's current content agrees;
        // retain old objects and bindings for existing installations and recovery.
        for member in s.skills.iter().filter(|x| x.source_id == source.id) {
            if member.bundle_digest != digest {
                let previous = root
                    .join("objects")
                    .join(&member.bundle_digest)
                    .join("tree");
                if member.external_path.is_some() || files::content_digest(&previous)? != digest {
                    return fail("此来源已有其他版本，请通过更新中心处理");
                }
            }
        }
        for member in s.skills.iter_mut().filter(|x| x.source_id == source.id) {
            if member.bundle_digest != digest {
                member.bundle_digest = digest.clone();
                member.version = digest[..12].to_string();
            }
        }
        s.sources.retain(|x| x.id != source.id);
        s.sources.push(source.clone());
        for item in &chosen {
            let original = PathBuf::from(&item.path);
            let relative = original
                .strip_prefix(&path)
                .map_err(|_| error::Error::Message("选择项不属于来源".into()))?
                .to_string_lossy()
                .replace('\\', "/");
            let existing = s
                .skills
                .iter()
                .find(|x| x.source_id == source.id && x.relative_path == relative)
                .cloned();
            let skill = existing.unwrap_or(Skill {
                external_path: None,
                id: id(),
                name: item.name.clone(),
                description: item.description.clone(),
                source_id: source.id.clone(),
                bundle_digest: digest.clone(),
                relative_path: relative,
                version: source.version.chars().take(12).collect(),
                installed_at: now(),
            });
            if !s.skills.iter().any(|x| x.id == skill.id) {
                s.skills.push(skill.clone());
            }
            if adopt && adoption_roots.contains(&item.path) {
                let parent = original
                    .parent()
                    .ok_or_else(|| error::Error::Message("不能接管系统根".into()))?;
                let target_id =
                    if let Some(t) = s.targets.iter().find(|t| Path::new(&t.path) == parent) {
                        t.id.clone()
                    } else {
                        let target = inferred_target(parent, &s.settings.agent_profiles);
                        let tid = target.id.clone();
                        s.targets.push(target);
                        tid
                    };
                if s.bindings.iter().any(|b| b.path == item.path) {
                    return fail("该位置已有受管记录，请先诊断");
                }
                // A sibling backup permits atomic rename even across volumes. Its location is journalled.
                let backup = parent.join(format!(".skilldock-backup-{}", id()));
                let backup_digest = files::manifest_digest(&original)?;
                if backup_digest != files::manifest_digest(&skill_path(root, &skill))? {
                    return fail("归集副本与原目录内容不一致，未替换原目录");
                }
                changes.push(Change {
                    restore: false,
                    backup_digest: Some(backup_digest),
                    path: original,
                    before: None,
                    after: Some(skill_path(root, &skill)),
                    backup: Some(backup),
                });
                s.bindings.push(Binding {
                    original_link: None,
                    external_path: None,
                    borrowed: false,
                    id: id(),
                    skill_id: skill.id.clone(),
                    target_id,
                    path: item.path.clone(),
                    version: skill.version.clone(),
                    digest: skill.bundle_digest.clone(),
                    relative_path: skill.relative_path.clone(),
                    claims: vec!["manual".into()],
                    follow: false,
                });
            }
        }
        Ok(())
    }

    pub(crate) fn import_batch(&self, r: &Value) -> Result<Snapshot> {
        let groups = r
            .get("groups")
            .and_then(Value::as_array)
            .filter(|groups| !groups.is_empty())
            .ok_or_else(|| error::Error::Message("请选择需要归集的 Skill".into()))?;
        self.transact(
            "import",
            "批量归集 Skill 到统一目录",
            |state, root, changes| {
                for group in groups {
                    let selected = strings(group, "selectedPaths")?;
                    if selected.is_empty() {
                        return fail("每个来源至少选择一个 Skill");
                    }
                    self.prepare_import(
                        &files::absolute(text(group, "path")?)?,
                        selected,
                        flag(r, "adopt"),
                        None,
                        state,
                        root,
                        changes,
                    )?;
                }
                for (index, change) in changes.iter().enumerate() {
                    if changes[..index].iter().any(|other| {
                        change.path.starts_with(&other.path) || other.path.starts_with(&change.path)
                    }) {
                        return fail("归集包含重复或交叉的原目录，请重新扫描");
                    }
                }
                Ok(())
            },
        )
    }
    pub(crate) fn execute_local(&self, action: &str, r: &Value) -> Result<Value> {
        if action == "import_batch" {
            return Ok(serde_json::to_value(self.import_batch(r)?)?);
        }
        if action == "import_folder" {
            return Ok(serde_json::to_value(self.import_folder(
                &files::absolute(text(r, "path")?)?,
                strings(r, "selectedPaths")?,
                flag(r, "adopt"),
                None,
            )?)?);
        }
        if action == "preview_snapshot_refresh" {
            let state = self.snapshot()?;
            let skill = lookup_skill(&state, text(r, "skillId")?)?;
            if skill.external_path.is_some() {
                return fail("本地引用无需重新收录快照");
            }
            let path = Path::new(&state.storage_root)
                .join("objects")
                .join(&skill.bundle_digest)
                .join("tree");
            return Ok(json!({
                "revision": state.revision,
                "contentDigest": files::content_digest(&path)?,
                "skillCount": state.skills.iter().filter(|item| item.bundle_digest == skill.bundle_digest && item.external_path.is_none()).count(),
                "path": path,
            }));
        }
        if action == "plan" {
            let s = self.snapshot()?;
            let claim = optional(r, "claim", "manual");
            let skills = if let Some(pid) = claim.strip_prefix("preset:") {
                let p = s
                    .presets
                    .iter()
                    .find(|p| p.id == pid)
                    .ok_or_else(|| error::Error::Message("预设不存在".into()))?;
                p.skill_ids
                    .iter()
                    .map(|id| {
                        p.locks
                            .get(id)
                            .cloned()
                            .map(Ok)
                            .unwrap_or_else(|| lookup_skill(&s, id).cloned())
                    })
                    .collect::<Result<Vec<_>>>()?
            } else if let Some(source_id) = claim.strip_prefix("source:") {
                Self::local_source_skills(&s, source_id)?
            } else {
                unique(strings(r, "skillIds")?)
                    .iter()
                    .map(|id| lookup_skill(&s, id).cloned())
                    .collect::<Result<Vec<_>>>()?
            };
            return Ok(serde_json::to_value(build_plan(
                &s,
                &skills,
                &unique(strings(r, "targetIds")?),
                claim,
                &[],
                flag(r, "adoptExisting"),
                &strings(r, "replaceBindingIds")?,
            )?)?);
        }
        if action == "diagnose" {
            let s = self.snapshot()?;
            let root = Path::new(&s.storage_root);
            let mut issues = vec![];
            for b in &s.bindings {
                if !binding_matches(root, b) {
                    issues.push(format!("链接异常：{}", b.path));
                } else if !Path::new(&b.path).join("SKILL.md").is_file() {
                    issues.push(format!("内容缺失：{}", b.path));
                }
            }
            for digest in s
                .skills
                .iter()
                .filter(|s| s.external_path.is_none())
                .map(|s| s.bundle_digest.clone())
                .collect::<BTreeSet<_>>()
            {
                match files::snapshot_matches(
                    &root.join("objects").join(&digest).join("tree"),
                    &digest,
                ) {
                    Ok(true) => {}
                    _ => issues.push(format!("快照已变化或丢失：{digest}")),
                }
            }
            return Ok(json!({"issues":issues}));
        }
        let state = self.transact(
            action,
            match action {
                "add_target" => "添加分发目标",
                "remove_update_source" => "移除更新管理（保留 Skill 与分发）",
                "remove_target" => "移除分发目标（保留文件）",
                "distribute" => "分发 Skill",
                "refresh_snapshot" => "重新收录统一库当前内容",
                "revoke" => "取消分发",
                "save_preset" => "保存预设",
                "apply_preset" => "整体分发预设",
                "revoke_preset" => "整体取消预设",
                "remove_skill" => "从库中卸载 Skill",
                "settings" => "保存设置",
                "rollback" => "回滚分发版本",
                _ => "更新管理记录",
            },
            |s, root, changes| {
                match action {
                    "remove_update_source" => {
                        if r.get("expectedRevision").and_then(Value::as_u64) != Some(s.revision as u64) {
                            return fail("数据已变化，请重新确认移除更新管理");
                        }
                        let source = s.sources.iter_mut().find(|source| source.id == optional(r, "sourceId", ""))
                            .ok_or_else(|| error::Error::Message("来源不存在".into()))?;
                        source.updates_removed = Some(true);
                        source.policy.mode = "off".into();
                        source.next_check.clear();
                    }
                    "remove_target" => {
                        if r.get("expectedRevision").and_then(Value::as_u64)
                            != Some(s.revision as u64)
                        {
                            return fail("数据已变化，请关闭弹窗后重新确认移除目标");
                        }
                        let target_id = text(r, "targetId")?;
                        let target = s
                            .targets
                            .iter()
                            .find(|t| t.id == target_id)
                            .cloned()
                            .ok_or_else(|| {
                                error::Error::Message("目标不存在，请刷新列表".into())
                            })?;
                        // Retained links must remain visible to object cleanup and migration checks.
                        if !s.unmanaged_target_paths.contains(&target.path) {
                            s.unmanaged_target_paths.push(target.path);
                        }
                        s.bindings.retain(|b| b.target_id != target_id);
                        s.preset_applications.retain(|a| a.target_id != target_id);
                        s.external_installations
                            .retain(|i| i.target_id != target_id);
                        s.targets.retain(|t| t.id != target_id);
                    }
                    "add_target" => {
                        let p = files::absolute(text(r, "path")?)?;
                        if files::protected(&p) || p.starts_with(root) || root.starts_with(&p) {
                            return fail("该路径不能作为分发目标");
                        }
                        fs::create_dir_all(&p)?;
                        let p = fs::canonicalize(p)?;
                        if p.starts_with(root) || root.starts_with(&p) {
                            return fail("目标与中央库不能互相包含");
                        }
                        if s.targets.iter().any(|t| Path::new(&t.path) == p) {
                            return fail("该物理目录已是分发目标，多个工具可能共享此位置");
                        }
                        let target = Target {
                            id: id(),
                            name: text(r, "name")?.trim().into(),
                            tool: optional(r, "tool", "custom").into(),
                            scope: optional(r, "scope", "project").into(),
                            path: p.display().to_string(),
                        };
                        restore_target_bindings(s, root, &target, changes)?;
                        s.targets.push(target);
                    }
                    "refresh_snapshot" => {
                        expected(s, r)?;
                        let skill = lookup_skill(s, text(r, "skillId")?)?.clone();
                        if skill.external_path.is_some() {
                            return fail("本地引用无需重新收录快照");
                        }
                        let old_digest = skill.bundle_digest;
                        let path = root.join("objects").join(&old_digest).join("tree");
                        if files::content_digest(&path)? != text(r, "contentDigest")? {
                            return fail("内容在预览后发生变化，请重新预览");
                        }
                        for member in s.skills.iter().filter(|item| {
                            item.bundle_digest == old_digest && item.external_path.is_none()
                        }) {
                            if !path.join(&member.relative_path).join("SKILL.md").is_file() {
                                return fail(format!(
                                    "{} 的 SKILL.md 缺失，不能重新收录",
                                    member.name
                                ));
                            }
                        }
                        let digest = files::snapshot_current_content(root, &path)?;
                        if digest != text(r, "contentDigest")? {
                            return fail("复制期间内容发生变化，请重新预览");
                        }
                        for member in s.skills.iter_mut().filter(|item| {
                            item.bundle_digest == old_digest && item.external_path.is_none()
                        }) {
                            member.bundle_digest = digest.clone();
                            member.version = digest[..12].to_string();
                        }
                        for source in s
                            .sources
                            .iter_mut()
                            .filter(|source| source.version == old_digest)
                        {
                            source.version = digest.clone();
                        }
                    }
                    "distribute" => {
                        expected(s, r)?;
                        let claim = optional(r, "claim", "manual");
                        if claim != "manual" {
                            return fail("预设请使用整体分发入口");
                        }
                        let ids = unique(strings(r, "skillIds")?);
                        let skills = ids
                            .iter()
                            .map(|id| lookup_skill(s, id).cloned())
                            .collect::<Result<Vec<_>>>()?;
                        distribute(
                            s,
                            root,
                            changes,
                            &skills,
                            &unique(strings(r, "targetIds")?),
                            claim,
                            flag(r, "takeover"),
                            flag(r, "adoptExisting"),
                            &strings(r, "replaceBindingIds")?,
                        )?;
                    }
                    "revoke" => {
                        let claim = optional(r, "claim", "manual");
                        if claim != "manual" {
                            return fail("预设分发请通过整体取消入口处理");
                        }
                        revoke(s, root, changes, &strings(r, "bindingIds")?, claim)?;
                    }
                    "save_preset" => {
                        let name = text(r, "name")?.trim();
                        if name.is_empty() {
                            return fail("请输入预设名称");
                        }
                        let skill_ids = unique(strings(r, "skillIds")?);
                        let locks = skill_ids
                            .iter()
                            .map(|i| Ok((i.clone(), lookup_skill(s, i)?.clone())))
                            .collect::<Result<BTreeMap<_, _>>>()?;
                        let pid = optional(r, "id", "");
                        let revision = s
                            .presets
                            .iter()
                            .find(|p| p.id == pid)
                            .map(|p| p.revision + 1)
                            .unwrap_or(1);
                        let pid = if pid.is_empty() {
                            id()
                        } else {
                            pid.to_string()
                        };
                        if let Some(value) = r.get("packages") {
                            let mut subscriptions: Vec<PresetPackage> =
                                serde_json::from_value(value.clone())?;
                            for subscription in &mut subscriptions {
                                subscription.preset_id = pid.clone();
                                let package = s
                                    .packages
                                    .iter()
                                    .find(|p| p.id == subscription.package_id)
                                    .ok_or_else(|| error::Error::Message("包不存在".into()))?;
                                if subscription
                                    .selected_ids
                                    .iter()
                                    .chain(&subscription.excluded_ids)
                                    .any(|id| !package.member_ids.contains(id))
                                {
                                    return fail("包成员已变化，请重新选择");
                                }
                                if subscription.auto_add {
                                    subscription.selected_ids = package
                                        .member_ids
                                        .iter()
                                        .filter(|id| !subscription.excluded_ids.contains(id))
                                        .cloned()
                                        .collect();
                                }
                                if subscription
                                    .selected_ids
                                    .iter()
                                    .any(|id| !skill_ids.contains(id))
                                {
                                    return fail("包选择与预设成员不一致，请重新保存");
                                }
                            }
                            s.preset_packages.retain(|p| p.preset_id != pid);
                            s.preset_packages.extend(subscriptions);
                        }
                        s.presets.retain(|p| p.id != pid);
                        s.presets.push(Preset {
                            id: pid,
                            name: name.into(),
                            description: optional(r, "description", "").into(),
                            skill_ids,
                            revision,
                            locks,
                        });
                    }
                    "apply_preset" => {
                        expected(s, r)?;
                        let preset = s
                            .presets
                            .iter()
                            .find(|p| p.id == optional(r, "presetId", ""))
                            .cloned()
                            .ok_or_else(|| error::Error::Message("预设不存在".into()))?;
                        let tids = unique(strings(r, "targetIds")?);
                        let claim = format!("preset:{}", preset.id);
                        let skills = preset
                            .skill_ids
                            .iter()
                            .map(|id| {
                                preset
                                    .locks
                                    .get(id)
                                    .or_else(|| s.skills.iter().find(|x| &x.id == id))
                                    .cloned()
                                    .ok_or_else(|| error::Error::Message("预设成员内容缺失".into()))
                            })
                            .collect::<Result<Vec<_>>>()?;
                        let replacements = strings(r, "replaceBindingIds")?;
                        // Keep selected source switches intact until their claims and links are validated.
                        let old = s
                            .bindings
                            .iter()
                            .filter(|b| {
                                tids.contains(&b.target_id)
                                    && b.claims.contains(&claim)
                                    && !preset.skill_ids.contains(&b.skill_id)
                                    && !replacements.contains(&b.id)
                            })
                            .map(|b| b.id.clone())
                            .collect::<Vec<_>>();
                        revoke(s, root, changes, &old, &claim)?;
                        distribute(
                            s,
                            root,
                            changes,
                            &skills,
                            &tids,
                            &claim,
                            flag(r, "takeover"),
                            flag(r, "adoptExisting"),
                            &strings(r, "replaceBindingIds")?,
                        )?;
                        for tid in &tids {
                            let follow =
                                r.get("follow").and_then(Value::as_bool).unwrap_or_else(|| {
                                    s.preset_applications
                                        .iter()
                                        .find(|a| a.preset_id == preset.id && &a.target_id == tid)
                                        .map(|a| a.follow)
                                        .unwrap_or(true)
                                });
                            s.preset_applications
                                .retain(|a| a.preset_id != preset.id || &a.target_id != tid);
                            s.preset_applications.push(PresetApplication {
                                preset_id: preset.id.clone(),
                                target_id: tid.clone(),
                                follow,
                                applied_revision: preset.revision,
                                error: String::new(),
                            });
                        }
                    }
                    "set_preset_follow" => {
                        let application = s
                            .preset_applications
                            .iter_mut()
                            .find(|a| {
                                a.preset_id == optional(r, "presetId", "")
                                    && a.target_id == optional(r, "targetId", "")
                            })
                            .ok_or_else(|| error::Error::Message("预设应用不存在".into()))?;
                        application.follow = flag(r, "follow");
                    }
                    "revoke_preset" => {
                        let claim = format!("preset:{}", text(r, "presetId")?);
                        let tids = strings(r, "targetIds")?;
                        if tids.is_empty() {
                            return fail("请选择要取消的目标");
                        }
                        let bids = s
                            .bindings
                            .iter()
                            .filter(|b| tids.contains(&b.target_id) && b.claims.contains(&claim))
                            .map(|b| b.id.clone())
                            .collect::<Vec<_>>();
                        revoke(s, root, changes, &bids, &claim)?;
                        s.preset_applications.retain(|a| {
                            a.preset_id != optional(r, "presetId", "")
                                || !tids.contains(&a.target_id)
                        });
                    }
                    "delete_preset" => {
                        let pid = text(r, "presetId")?;
                        if s.bindings
                            .iter()
                            .any(|b| b.claims.contains(&format!("preset:{pid}")))
                        {
                            return fail("预设仍在分发，请先取消");
                        }
                        s.presets.retain(|p| p.id != pid);
                        s.preset_packages.retain(|p| p.preset_id != pid);
                        s.preset_applications.retain(|p| p.preset_id != pid);
                    }
                    "remove_skill" => {
                        let sid = text(r, "skillId")?;
                        if s.sources.iter().any(|source| source.local_member_ids.as_ref().is_some_and(|ids| ids.iter().any(|id| id == sid))) {
                            return fail("Skill 仍属于本地来源，请先在来源中调整成员");
                        }
                        if s.packages
                            .iter()
                            .any(|p| p.member_ids.iter().any(|id| id == sid))
                        {
                            return fail("成员仍属于同步包，请在包中处理成员变更");
                        }
                        if s.bindings.iter().any(|b| b.skill_id == sid)
                            || s.presets
                                .iter()
                                .any(|p| p.skill_ids.iter().any(|i| i == sid))
                        {
                            return fail("Skill 仍被目标或预设引用，请先解除引用");
                        }
                        s.skills.retain(|x| x.id != sid);
                    }
                    "set_policy" => {
                        let source = s
                            .sources
                            .iter_mut()
                            .find(|x| x.id == optional(r, "sourceId", ""))
                            .ok_or_else(|| error::Error::Message("来源不存在".into()))?;
                        if source.updates_removed == Some(true) {
                            return fail("此来源已移除更新管理");
                        }
                        if source.kind == "local_reference" {
                            return fail("本地引用直接跟随原目录，无需定时更新");
                        }
                        let mode = text(r, "mode")?;
                        if !["off", "notify", "auto"].contains(&mode) {
                            return fail("无效更新模式");
                        }
                        if !source.supports_remote_updates() && mode != "off" {
                            return fail("仅从远程 Git 或网站获取的来源支持定时更新；本地文件夹请手动扫描并确认同步");
                        }
                        let hours = r.get("intervalHours").and_then(Value::as_u64).unwrap_or(24);
                        if !(1..=8760).contains(&hours) {
                            return fail("更新间隔应为 1 至 8760 小时");
                        }
                        let daily_time = match r.get("dailyTime") {
                            None | Some(Value::Null) => None,
                            Some(Value::String(value)) if crate::schedule::valid_daily_time(value) => Some(value.clone()),
                            _ => return fail("每天执行时间应为 HH:mm（00:00 至 23:59）"),
                        };
                        source.policy = Policy {
                            mode: mode.into(),
                            interval_hours: hours as u32,
                            daily_time,
                        };
                        source.next_check = crate::schedule::next_check(&source.policy);
                    }
                    "set_follow" => {
                        let b = s
                            .bindings
                            .iter_mut()
                            .find(|b| b.id == optional(r, "bindingId", ""))
                            .ok_or_else(|| error::Error::Message("分发不存在".into()))?;
                        if b.external_path.is_some() {
                            return fail("本地引用跟随原目录，不能固定为快照版本");
                        }
                        if flag(r, "follow") && b.claims.iter().any(|c| c != "manual") {
                            return fail("预设锁定的目标通过新预设修订更新");
                        }
                        b.follow = flag(r, "follow");
                    }
                    "settings" => {
                        if let Some(value) = r.get("networkProxy") {
                            let proxy: NetworkProxy = serde_json::from_value(value.clone())?;
                            network::validate_proxy(&proxy)?;
                            s.settings.network_proxy = proxy;
                        }
                        if let Some(value) = r.get("backupRetention") {
                            let count = value
                                .as_u64()
                                .filter(|n| (1..=100).contains(n))
                                .ok_or_else(|| {
                                    error::Error::Message("备份保留次数应为 1–100 的整数".into())
                                })?;
                            s.settings.backup_retention = count as u32;
                        }
                        if let Some(value) = r.get("agentProfiles") {
                            let profiles = parse_agent_profiles(value)?;
                            for profile in &profiles {
                                for path in &profile.user_paths {
                                    let path = files::absolute(path)?;
                                    let path = fs::canonicalize(&path).unwrap_or(path);
                                    if path.starts_with(root) || root.starts_with(&path) {
                                        return fail("工具目录不能与中央库互相包含");
                                    }
                                }
                            }
                            s.settings.agent_profiles = profiles;
                        }
                        if r.get("catalogSites").is_some() {
                            let sites = parse_catalog_sites(&r["catalogSites"])?;
                            s.settings.catalog_sites = sites;
                        }

                        if let Some(v) = r.get("closeToTray").and_then(Value::as_bool) {
                            s.settings.close_to_tray = v;
                        }
                        if let Some(v) = r.get("theme").and_then(Value::as_str) {
                            if !["light", "dark", "system"].contains(&v) {
                                return fail("无效主题");
                            }
                            s.settings.theme = v.into();
                        }
                        for key in ["updateEndpoint", "updatePublicKey", "updateMode"] {
                            if let Some(v) = r.get(key).and_then(Value::as_str) {
                                match key {
                                    "updateEndpoint" => {
                                        if !v.is_empty() && !v.starts_with("https://") {
                                            return fail("升级源需要 HTTPS");
                                        }
                                        s.settings.update_endpoint = v.into()
                                    }
                                    "updatePublicKey" => s.settings.update_public_key = v.into(),
                                    _ => {
                                        if !["off", "notify", "auto"].contains(&v) {
                                            return fail("更新策略无效");
                                        }
                                        s.settings.update_mode = v.into()
                                    }
                                }
                            }
                        }
                    }
                    "rollback" => {
                        let b = s
                            .bindings
                            .iter_mut()
                            .find(|b| b.id == optional(r, "bindingId", ""))
                            .ok_or_else(|| error::Error::Message("分发不存在".into()))?;
                        if b.claims.iter().any(|c| c != "manual") {
                            return fail("该链接由预设锁定，请通过预设修订回滚");
                        }
                        if b.external_path.is_some() {
                            return fail("本地引用没有历史快照，不能回滚");
                        }
                        let digest = text(r, "digest")?;
                        if digest.len() != 64 || !digest.chars().all(|c| c.is_ascii_hexdigit()) {
                            return fail("版本摘要无效");
                        }
                        let dest = root
                            .join("objects")
                            .join(digest)
                            .join("tree")
                            .join(&b.relative_path);
                        if !dest.join("SKILL.md").is_file()
                            || !files::snapshot_matches(
                                &root.join("objects").join(digest).join("tree"),
                                digest,
                            )?
                        {
                            return fail("指定历史快照不存在或损坏");
                        }
                        changes.push(Change {
                            restore: false,
                            backup_digest: None,
                            path: PathBuf::from(&b.path),
                            before: Some(binding_path(root, b)),
                            after: Some(dest),
                            backup: None,
                        });
                        b.digest = digest.into();
                        b.version = digest[..12].into();
                        b.follow = false;
                    }
                    _ => return fail(format!("未知操作：{action}")),
                }
                Ok(())
            },
        )?;
        Ok(serde_json::to_value(state)?)
    }
    fn source_failure(&self, source_id: &str, message: String) -> error::Error {
        let result = self.transact("update_error", "来源检查失败", |s, _, _| {
            if let Some(src) = s.sources.iter_mut().find(|x| x.id == source_id) {
                src.status = "error".into();
                src.error = message.clone();
                src.last_checked = now();
                src.next_check = crate::schedule::next_check(&src.policy);
            }
            Ok(())
        });
        error::Error::Message(match result {
            Ok(_) => message,
            Err(e) => format!("{message}；状态记录失败：{e}"),
        })
    }
    pub async fn check_source(&self, source_id: &str, apply: bool) -> Result<Snapshot> {
        let proxy = self.snapshot()?.settings.network_proxy;
        network::with_proxy(proxy, self.check_source_with_proxy(source_id, apply)).await
    }
    async fn check_source_with_proxy(&self, source_id: &str, apply: bool) -> Result<Snapshot> {
        let baseline = self.snapshot()?;
        let source = baseline
            .sources
            .iter()
            .find(|s| s.id == source_id)
            .cloned()
            .ok_or_else(|| error::Error::Message("来源不存在".into()))?;
        if source.updates_removed == Some(true) {
            return fail("此来源已移除更新管理");
        }
        if source.kind == "local_reference" {
            return Ok(baseline);
        }
        if source.status == "detached" {
            return fail(
                "此归集记录没有独立更新来源，请从实际 Git 仓库或源码包导入并核对成员；当前 Skill 仍可正常使用",
            );
        }
        let prepared = match source.kind.as_str() {
            "git" => {
                network::prepare_git(
                    &Path::new(&baseline.storage_root).join("cache"),
                    &source.url,
                    &source.reference,
                    &source.scan_subdir,
                )
                .await
            }
            "catalog" | "clawhub" => {
                network::prepare_catalog(
                    &Path::new(&baseline.storage_root).join("cache"),
                    &source.reference,
                    &source.url,
                )
                .await
            }
            "local" => Ok(network::PreparedSource {
                scan_subdir: source.scan_subdir.clone(),
                path: PathBuf::from(&source.path),
                kind: "local".into(),
                name: source.name.clone(),
                url: String::new(),
                reference: String::new(),
                version: String::new(),
            }),
            _ => return fail("该来源尚不支持自动更新"),
        };
        let prepared = prepared.map_err(|e| self.source_failure(source_id, e.to_string()))?;
        let original_path = fs::canonicalize(&prepared.path)
            .map_err(|e| self.source_failure(source_id, e.to_string()))?;
        if source.kind == "local"
            && (original_path.starts_with(&baseline.storage_root)
                || Path::new(&baseline.storage_root).starts_with(&original_path))
        {
            return fail("本地来源与统一库互相包含，拒绝更新");
        }
        let dependency_issues = files::python_environment_issues(&original_path)?;
        let local_view = if source.kind == "local" {
            self.local_source_view(&original_path, true)
                .map_err(|e| self.source_failure(source_id, e.to_string()))?
        } else {
            Self::remote_source_view(&original_path)
                .map_err(|e| self.source_failure(source_id, e.to_string()))?
        };
        let path = local_view
            .as_ref()
            .map(|v| v.path().to_path_buf())
            .unwrap_or(original_path);
        if path.starts_with(&baseline.storage_root) && source.kind == "local"
            || source.kind == "local" && Path::new(&baseline.storage_root).starts_with(&path)
        {
            return fail("本地来源与统一库互相包含，拒绝更新");
        }
        let digest = files::content_digest(&path)
            .map_err(|e| self.source_failure(source_id, e.to_string()))?;
        let scanned = files::scan(&path.join(&source.scan_subdir))
            .map_err(|e| self.source_failure(source_id, e.to_string()))?;
        let version = if prepared.version.is_empty() {
            digest.clone()
        } else {
            prepared.version.clone()
        };
        let updated = self.transact(
            "update",
            if apply {
                "更新来源内容"
            } else {
                "检查来源更新"
            },
            |s, root, changes| {
                if s.storage_root != baseline.storage_root {
                    return fail("下载期间存储位置已变化，请重试");
                }
                let src = s
                    .sources
                    .iter()
                    .find(|x| x.id == source_id)
                    .ok_or_else(|| error::Error::Message("来源已移除".into()))?;
                if src.updates_removed == Some(true) {
                    return fail("检查期间此来源已移除更新管理");
                }
                if src.version != source.version
                    || src.kind != source.kind
                    || src.url != source.url
                    || src.reference != source.reference
                    || src.scan_subdir != source.scan_subdir
                    || src.policy.mode != source.policy.mode
                    || src.path != source.path
                {
                    return fail("检查期间来源或策略已变化，请重试");
                }
                if apply {
                    for package in s
                        .packages
                        .iter_mut()
                        .filter(|p| p.scopes.iter().any(|scope| scope.source_id == source_id))
                    {
                        let mut issues = dependency_issues.clone();
                        if !package.remote && Path::new(&package.path).is_dir() {
                            issues.extend(files::python_environment_issues(Path::new(
                                &package.path,
                            ))?);
                        }
                        issues.sort();
                        issues.dedup();
                        package.issues = issues;
                    }
                }
                let changed = s
                    .skills
                    .iter()
                    .any(|x| x.source_id == source_id && x.bundle_digest != digest)
                    || scanned
                        .items
                        .iter()
                        .filter(|item| item.status == "ready")
                        .any(|item| {
                            let relative = Path::new(&item.path)
                                .strip_prefix(&path)
                                .unwrap()
                                .to_string_lossy()
                                .replace('\\', "/");
                            Self::package_accepts_new(s, source_id, &relative)
                                && !s.skills.iter().any(|skill| {
                                    skill.source_id == source_id && skill.relative_path == relative
                                })
                        });
                let mut status = if changed { "available" } else { "current" }.to_string();
                let mut details = Vec::new();
                if !changed && !details.is_empty() {
                    status = "attention".into();
                }
                if changed && apply {
                    details.clear();
                    if files::snapshot_current_content(root, &path)? != digest {
                        return fail("来源内容在同步期间发生变化，请重试");
                    }
                    let entries = scanned
                        .items
                        .iter()
                        .filter(|x| x.status == "ready")
                        .map(|x| {
                            (
                                Path::new(&x.path)
                                    .strip_prefix(&path)
                                    .unwrap()
                                    .to_string_lossy()
                                    .replace('\\', "/"),
                                x,
                            )
                        })
                        .collect::<BTreeMap<_, _>>();
                    for skill in s.skills.iter_mut().filter(|x| x.source_id == source_id) {
                        if let Some(item) = entries.get(&skill.relative_path) {
                            if item.name != skill.name {
                                details.push(format!("{} 名称变化，待审阅", skill.name));
                                continue;
                            }
                            skill.bundle_digest = digest.clone();
                            skill.version = version.chars().take(12).collect();
                            skill.description = item.description.clone();
                        } else {
                            details.push(format!("{} 已从来源移除，保留旧安装", skill.name));
                        }
                    }
                    let additions: Vec<_> = entries
                        .iter()
                        .filter(|(rel, _)| {
                            Self::package_accepts_new(s, source_id, rel)
                                && !s
                                    .skills
                                    .iter()
                                    .any(|x| x.source_id == source_id && &x.relative_path == *rel)
                        })
                        .map(|(rel, item)| Skill {
                            id: id(),
                            name: item.name.clone(),
                            description: item.description.clone(),
                            source_id: source_id.into(),
                            bundle_digest: digest.clone(),
                            relative_path: rel.clone(),
                            version: version.chars().take(12).collect(),
                            installed_at: now(),
                            external_path: None,
                        })
                        .collect();
                    s.skills.extend(additions);
                    let missing: Vec<_> = s
                        .skills
                        .iter()
                        .filter(|m| {
                            m.source_id == source_id && !entries.contains_key(&m.relative_path)
                        })
                        .map(|m| m.id.clone())
                        .collect();
                    let source_ids: Vec<_> = s
                        .skills
                        .iter()
                        .filter(|m| m.source_id == source_id)
                        .map(|m| m.id.clone())
                        .collect();
                    for package in &mut s.packages {
                        package
                            .missing_member_ids
                            .retain(|id| !source_ids.contains(id));
                        package.missing_member_ids.extend(
                            missing
                                .iter()
                                .filter(|id| package.member_ids.contains(id))
                                .cloned(),
                        );
                    }
                    Self::update_package_presets(s, source_id);
                    let new_count = entries
                        .keys()
                        .filter(|rel| {
                            !Self::package_accepts_new(s, source_id, rel)
                                && !s
                                    .skills
                                    .iter()
                                    .any(|x| x.source_id == source_id && &x.relative_path == *rel)
                        })
                        .count();
                    if new_count > 0 {
                        details.push(format!("发现 {new_count} 个新成员，需手动选择入库"));
                    }
                    status = if details.is_empty() {
                        "current"
                    } else {
                        "attention"
                    }
                    .into();
                }
                if apply {
                    for binding in &mut s.bindings {
                        if binding.follow
                            && binding.claims.iter().all(|c| c == "manual")
                            && let Some(skill) = s.skills.iter().find(|x| {
                                x.id == binding.skill_id
                                    && x.source_id == source_id
                                    && x.bundle_digest != binding.digest
                            })
                        {
                            if s.packages
                                .iter()
                                .any(|p| p.member_ids.contains(&skill.id) && !p.issues.is_empty())
                            {
                                continue;
                            }
                            changes.push(Change {
                                restore: false,
                                backup_digest: None,
                                path: PathBuf::from(&binding.path),
                                before: Some(binding_path(root, binding)),
                                after: Some(skill_path(root, skill)),
                                backup: None,
                            });
                            binding.digest = skill.bundle_digest.clone();
                            binding.version = skill.version.clone();
                        }
                    }
                }
                // Pending member and dependency differences survive repeated checks,
                // and clear when the source actually resolves them.
                details.clear();
                let current_entries: BTreeMap<_, _> = scanned
                    .items
                    .iter()
                    .filter(|i| i.status == "ready")
                    .map(|i| {
                        (
                            Path::new(&i.path)
                                .strip_prefix(&path)
                                .unwrap()
                                .to_string_lossy()
                                .replace('\\', "/"),
                            i,
                        )
                    })
                    .collect();
                for skill in s.skills.iter().filter(|i| i.source_id == source_id) {
                    match current_entries.get(&skill.relative_path) {
                        Some(item) if item.name != skill.name => {
                            details.push(format!("{} 名称变化，待审阅", skill.name))
                        }
                        None => details.push(format!("{} 已从来源移除，保留旧安装", skill.name)),
                        _ => (),
                    }
                }
                let pending_count = current_entries
                    .keys()
                    .filter(|rel| {
                        !s.packages
                            .iter()
                            .any(|p| p.scopes.iter().any(|scope| scope.source_id == source_id))
                            && !s
                                .skills
                                .iter()
                                .any(|i| i.source_id == source_id && &i.relative_path == *rel)
                    })
                    .count();
                if pending_count > 0 {
                    details.push(format!("发现 {pending_count} 个新成员，需手动选择入库"));
                }
                details.extend(
                    s.packages
                        .iter()
                        .filter(|p| p.scopes.iter().any(|scope| scope.source_id == source_id))
                        .flat_map(|p| p.issues.clone()),
                );
                if apply || !changed {
                    status = if details.is_empty() {
                        "current"
                    } else {
                        "attention"
                    }
                    .into();
                }
                let src = s.sources.iter_mut().find(|x| x.id == source_id).unwrap();
                src.last_checked = now();
                src.next_check = crate::schedule::next_check(&src.policy);
                src.status = status;
                src.error = details.join("；");
                if apply {
                    src.version = version;
                }
                Ok(())
            },
        )?;
        if apply {
            self.reconcile_packages()?;
            return self.snapshot();
        }
        Ok(updated)
    }
    pub async fn run_due_updates(&self) -> Result<()> {
        let state = self.snapshot()?;
        if !state.initialized {
            return Ok(());
        }
        for source in state.sources {
            if source.supports_remote_updates()
                && source.policy.mode != "off"
                && chrono::DateTime::parse_from_rfc3339(&source.next_check)
                    .map(|d| d <= chrono::Utc::now())
                    .unwrap_or(true)
                && let Err(e) = self
                    .check_source(&source.id, source.policy.mode == "auto")
                    .await
            {
                tracing::warn!(source_id=%source.id,error=%e,"scheduled_update_failed");
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod catalog_site_tests {
    use super::*;
    #[test]
    fn legacy_sites_upgrade_without_losing_custom_urls() {
        let sites = parse_catalog_sites(&serde_json::json!([
            "clawhub",
            "https://example.org/catalog"
        ]))
        .unwrap();
        assert_eq!(sites[0].name, "ClawHub");
        assert_eq!(sites[0].url, "https://clawhub.ai");
        assert_eq!(sites[1].url, "https://example.org/catalog");
        let encoded = serde_json::to_value(&sites).unwrap();
        assert!(encoded[0].is_object());
    }
    #[test]
    fn named_sites_roundtrip_and_trim() {
        let sites = parse_catalog_sites(
            &serde_json::json!([{"name":" 团队技能 ","url":"https://example.org/"}]),
        )
        .unwrap();
        assert_eq!(sites[0].name, "团队技能");
        assert_eq!(sites[0].url, "https://example.org");
        let again: Vec<CatalogSite> =
            serde_json::from_value(serde_json::to_value(&sites).unwrap()).unwrap();
        assert_eq!(again[0].name, sites[0].name);
    }
    #[test]
    fn invalid_and_duplicate_sites_are_rejected() {
        for value in [
            serde_json::json!([]),
            serde_json::json!([{"name":" ","url":"https://example.org"}]),
            serde_json::json!([{"name":"Test","url":"http://example.org"}]),
            serde_json::json!([{"name":"Test","url":"https://user:pass@example.org"}]),
            serde_json::json!([{"name":"Test","url":"https://example.org?q=x"}]),
            serde_json::json!(["skillhub", "https://api.skillhub.cn/"]),
            serde_json::json!(vec!["clawhub"; 9]),
        ] {
            assert!(parse_catalog_sites(&value).is_err(), "{value}");
        }
    }
}

#[cfg(test)]
mod agent_profile_tests {
    use super::*;
    #[test]
    fn defaults_include_requested_agents_and_multiple_paths() {
        let profiles = default_agent_profiles();
        for id in ["workbuddy", "trae", "antigravity"] {
            assert!(profiles.iter().any(|p| p.id == id));
        }
        assert!(
            profiles
                .iter()
                .find(|p| p.id == "antigravity")
                .unwrap()
                .user_paths
                .len()
                > 1
        );
        assert!(parse_agent_profiles(&serde_json::to_value(profiles).unwrap()).is_ok());
    }
    #[test]
    fn legacy_settings_gain_profiles_and_shared_paths_are_allowed() {
        let mut old = serde_json::to_value(Settings::default()).unwrap();
        old.as_object_mut().unwrap().remove("agentProfiles");
        let restored: Settings = serde_json::from_value(old).unwrap();
        assert_eq!(
            restored.agent_profiles.len(),
            default_agent_profiles().len()
        );
        assert!(
            parse_agent_profiles(&serde_json::json!([
                {"id":"a","name":"A","userPaths":["~/.agents/skills"],"projectPaths":[]},
                {"id":"b","name":"B","userPaths":["~/.agents/skills"],"projectPaths":[]}
            ]))
            .is_ok()
        );
    }
    #[test]
    fn invalid_directory_templates_are_rejected() {
        for paths in [
            serde_json::json!(["../skills"]),
            serde_json::json!(["/tmp/skills"]),
            serde_json::json!(["."]),
            serde_json::json!([""]),
        ] {
            assert!(
                parse_agent_profiles(
                    &serde_json::json!([{"id":"a","name":"A","userPaths":[],"projectPaths":paths}])
                )
                .is_err()
            );
        }
    }
}

#[cfg(test)]
mod nested_import_tests {
    use super::*;

    fn setup() -> (tempfile::TempDir, Engine, PathBuf, PathBuf) {
        let temp = tempfile::tempdir().unwrap();
        let parent = temp.path().join("parent");
        let child = parent.join("child");
        fs::create_dir_all(child.join("grandchild")).unwrap();
        for dir in [&parent, &child, &child.join("grandchild")] {
            fs::write(
                dir.join("SKILL.md"),
                format!("# {}", dir.file_name().unwrap().to_string_lossy()),
            )
            .unwrap();
        }
        let engine = Engine::new(Some(temp.path().join("config"))).unwrap();
        engine
            .configure(temp.path().join("library").to_str().unwrap())
            .unwrap();
        (temp, engine, parent, child)
    }

    #[cfg(unix)]
    #[test]
    fn nested_adoption_links_only_outermost_and_survives_library_migration() {
        let (_temp, engine, parent, child) = setup();
        let state = engine.import_folder(&parent, vec![], true, None).unwrap();
        assert_eq!(state.skills.len(), 3);
        assert_eq!(state.bindings.len(), 1);
        assert!(
            fs::symlink_metadata(&parent)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert!(
            !fs::symlink_metadata(&child)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        for skill in &state.skills {
            assert!(
                skill_path(Path::new(&state.storage_root), skill)
                    .join("SKILL.md")
                    .is_file()
            );
        }
        let old = PathBuf::from(&state.storage_root);
        let destination = tempfile::tempdir().unwrap();
        let state = engine
            .migrate_storage(destination.path().join("moved").to_str().unwrap())
            .unwrap();
        assert!(!old.exists());
        assert_eq!(
            fs::read_link(&parent).unwrap(),
            binding_path(Path::new(&state.storage_root), &state.bindings[0])
        );
        assert!(child.join("grandchild/SKILL.md").is_file());
    }

    #[cfg(unix)]
    #[test]
    fn selecting_only_child_preserves_parent_directory() {
        let (_temp, engine, parent, child) = setup();
        let selected = fs::canonicalize(&child).unwrap().display().to_string();
        let state = engine
            .import_folder(&parent, vec![selected], true, None)
            .unwrap();
        assert_eq!(state.skills.len(), 1);
        assert_eq!(state.bindings.len(), 1);
        assert!(
            !fs::symlink_metadata(&parent)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert!(
            fs::symlink_metadata(&child)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert!(parent.join("SKILL.md").is_file());
    }

    #[test]
    fn copying_nested_skills_does_not_replace_directories() {
        let (_temp, engine, parent, child) = setup();
        let state = engine.import_folder(&parent, vec![], false, None).unwrap();
        assert_eq!(state.skills.len(), 3);
        assert!(state.bindings.is_empty());
        assert!(
            !fs::symlink_metadata(&parent)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert!(
            !fs::symlink_metadata(&child)
                .unwrap()
                .file_type()
                .is_symlink()
        );
    }
}

#[cfg(test)]
mod target_name_tests {
    use super::*;
    #[test]
    fn names_global_project_and_unknown_skill_directories() {
        let profiles = default_agent_profiles();
        let codex = files::absolute("~/.codex/skills").unwrap();
        assert_eq!(inferred_target(&codex, &profiles).name, "Codex · 用户级");
        let project = inferred_target(Path::new("/workspace/design/.cursor/skills"), &profiles);
        assert_eq!(project.name, "Cursor · design");
        assert_eq!(project.scope, "project");
        let other = inferred_target(Path::new("/workspace/team/skills"), &profiles);
        assert_eq!(other.name, "team · Skills");
        assert_eq!(other.tool, "custom");
    }
}

#[cfg(test)]
mod remote_view_tests {
    use super::*;
    #[test]
    fn remote_git_metadata_does_not_change_skill_version_or_drop_dependencies() {
        let checkout = tempfile::tempdir().unwrap();
        fs::create_dir_all(checkout.path().join(".git/logs")).unwrap();
        fs::create_dir_all(checkout.path().join("node_modules/p")).unwrap();
        fs::write(checkout.path().join("SKILL.md"), "# Remote").unwrap();
        fs::write(checkout.path().join("node_modules/p/index.js"), "runtime").unwrap();
        fs::write(checkout.path().join(".git/logs/HEAD"), "clone-one").unwrap();
        let first = Engine::remote_source_view(checkout.path())
            .unwrap()
            .unwrap();
        let digest = files::tree_digest(first.path()).unwrap();
        fs::write(checkout.path().join(".git/logs/HEAD"), "clone-two").unwrap();
        let second = Engine::remote_source_view(checkout.path())
            .unwrap()
            .unwrap();
        assert_eq!(files::tree_digest(second.path()).unwrap(), digest);
        assert!(second.path().join("node_modules/p/index.js").is_file());
        assert!(!second.path().join(".git").exists());
        assert!(checkout.path().join(".git").is_dir());
    }
}
