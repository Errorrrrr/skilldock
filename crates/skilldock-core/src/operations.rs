use crate::{
    error::{Result, fail},
    files::{self, Change},
    *,
};
use std::collections::{BTreeMap, BTreeSet};

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
) -> Result<DistributionPlan> {
    if skills.is_empty() || targets.is_empty() {
        return fail("请选择 Skill 和目标");
    }
    let root = Path::new(&s.storage_root);
    let mut items = vec![];
    let mut paths = BTreeSet::new();
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
            let path = Path::new(&target.path).join(files::clean_name(&skill.name)?);
            let mut item = PlanItem {
                skill_id: skill.id.clone(),
                target_id: target.id.clone(),
                path: path.display().to_string(),
                action: "create".into(),
                error: String::new(),
            };
            if !paths.insert(item.path.to_lowercase()) {
                item.error = "所选成员存在同名路径，请分开处理".into();
            }
            if !skill_path(root, skill).join("SKILL.md").is_file() {
                item.error = "中央库内容缺失".into();
            }
            if let Some(b) = s.bindings.iter().find(|b| b.path == item.path) {
                item.action = "reuse".into();
                if fs::read_link(&path).ok() != Some(binding_path(root, b)) {
                    item.error = "已有链接已被外部修改或丢失".into();
                }
                if b.skill_id != skill.id {
                    item.error = "目标存在同名的其他 Skill".into();
                } else if b.digest != skill.bundle_digest || b.relative_path != skill.relative_path
                {
                    item.action = "update".into();
                    if b.claims.iter().any(|c| c != claim) {
                        item.error = "其他预设或手动分发仍锁定旧版本".into();
                    }
                }
            } else if files::exists(&path) {
                item.error = "目标存在非本工具管理的内容，不覆盖".into();
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
fn distribute(
    s: &mut Snapshot,
    root: &Path,
    changes: &mut Vec<Change>,
    skills: &[Skill],
    targets: &[String],
    claim: &str,
) -> Result<()> {
    let plan = build_plan(s, skills, targets, claim)?;
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
    let digests: BTreeSet<_> = skills.iter().map(|x| x.bundle_digest.clone()).collect();
    for digest in digests {
        if files::tree_digest(&root.join("objects").join(&digest).join("tree"))? != digest {
            return fail("快照被外部修改，请先诊断");
        }
    }
    for item in plan.items {
        let skill = skills.iter().find(|x| x.id == item.skill_id).unwrap();
        if let Some(b) = s.bindings.iter_mut().find(|b| b.path == item.path) {
            if item.action == "update" {
                changes.push(Change {
                    path: PathBuf::from(&b.path),
                    before: Some(binding_path(root, b)),
                    after: Some(skill_path(root, skill)),
                    backup: None,
                });
                b.digest = skill.bundle_digest.clone();
                b.version = skill.version.clone();
                b.relative_path = skill.relative_path.clone();
            }
            if !b.claims.iter().any(|c| c == claim) {
                b.claims.push(claim.into());
            }
        } else {
            changes.push(Change {
                path: PathBuf::from(&item.path),
                before: None,
                after: Some(skill_path(root, skill)),
                backup: None,
            });
            s.bindings.push(Binding {
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
fn revoke(
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
        if b.claims.is_empty() {
            let path = PathBuf::from(&b.path);
            if files::exists(&path) {
                if fs::read_link(&path).ok() != Some(binding_path(root, b)) {
                    return fail("目标已被其他内容替换，拒绝取消");
                }
                changes.push(Change {
                    path,
                    before: Some(binding_path(root, b)),
                    after: None,
                    backup: None,
                });
            }
        }
    }
    s.bindings.retain(|b| !b.claims.is_empty());
    Ok(())
}

impl Engine {
    // Existing agent roots often mix real directories and installations linked by other managers.
    // Exclude those top-level installation links without dereferencing or changing them.
    fn local_source_view(&self, path: &Path) -> Result<Option<tempfile::TempDir>> {
        if path.join("SKILL.md").is_file() {
            return Ok(None);
        }
        let known = Self::discover()?
            .iter()
            .chain(self.snapshot()?.targets.iter())
            .any(|t| fs::canonicalize(&t.path).ok().as_deref() == Some(path));
        if !known {
            return Ok(None);
        }
        let mut excluded = vec![];
        for entry in fs::read_dir(path)? {
            let e = entry?;
            if e.file_type()?.is_symlink()
                && (!e.path().exists() || e.path().join("SKILL.md").is_file())
            {
                excluded.push(e.path());
            }
        }
        if excluded.is_empty() {
            return Ok(None);
        }
        let temp = tempfile::tempdir()?;
        files::copy_tree_excluding(path, temp.path(), &excluded)?;
        Ok(Some(temp))
    }
    pub(crate) fn import_folder(
        &self,
        path: &Path,
        selected: Vec<String>,
        adopt: bool,
        source_info: Option<Source>,
    ) -> Result<Snapshot> {
        let path = files::absolute(&path.display().to_string())?;
        if files::protected(&path) || fs::symlink_metadata(&path)?.file_type().is_symlink() {
            return fail("该目录由其他工具管理或是软链，不能接管为来源");
        }
        let path = fs::canonicalize(path)?;
        let scanned = files::scan(&path)?;
        let local_view = if source_info.is_none() {
            self.local_source_view(&path)?
        } else {
            None
        };
        let snapshot_source = local_view.as_ref().map(|v| v.path()).unwrap_or(&path);
        let mut chosen: Vec<_> = scanned
            .items
            .into_iter()
            .filter(|i| i.status == "ready" && (selected.is_empty() || selected.contains(&i.path)))
            .collect();
        if chosen.is_empty() {
            return fail("未发现可导入的 Skill");
        }
        chosen.sort_by_key(|i| i.path.len());
        if adopt
            && chosen.iter().enumerate().any(|(i, a)| {
                chosen
                    .iter()
                    .skip(i + 1)
                    .any(|b| Path::new(&b.path).starts_with(&a.path))
            })
        {
            return fail("嵌套 Skill 不能同时替换原目录，请分批处理");
        }
        self.transact(
            "import",
            "导入 Skill 到统一目录",
            |s, root, changes| {
                if root.starts_with(&path)
                    || (path.starts_with(root)
                        && !(source_info.is_some() && path.starts_with(root.join("cache"))))
                {
                    return fail("来源目录不能与中央库互相包含");
                }
                let digest = files::snapshot_tree(root, snapshot_source)?;
                let mut source = source_info.unwrap_or(Source {
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
                        "原安装位置已归集；更新前请重新绑定原始来源"
                    } else {
                        ""
                    }
                    .into(),
                });
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
                    source.policy = old.policy.clone();
                }
                if s.sources.iter().any(|x| x.id == source.id)
                    && s.skills
                        .iter()
                        .any(|x| x.source_id == source.id && x.bundle_digest != digest)
                {
                    return fail("此来源已有其他版本，请通过更新中心处理");
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
                    if adopt {
                        let parent = original
                            .parent()
                            .ok_or_else(|| error::Error::Message("不能接管系统根".into()))?;
                        let target_id = if let Some(t) =
                            s.targets.iter().find(|t| Path::new(&t.path) == parent)
                        {
                            t.id.clone()
                        } else {
                            let tid = id();
                            s.targets.push(Target {
                                id: tid.clone(),
                                name: parent
                                    .file_name()
                                    .unwrap_or_default()
                                    .to_string_lossy()
                                    .into(),
                                tool: "custom".into(),
                                scope: "user".into(),
                                path: parent.display().to_string(),
                            });
                            tid
                        };
                        if s.bindings.iter().any(|b| b.path == item.path) {
                            return fail("该位置已有受管记录，请先诊断");
                        }
                        // A sibling backup permits atomic rename even across volumes. Its location is journalled.
                        let backup = parent.join(format!(".skilldock-backup-{}", id()));
                        changes.push(Change {
                            path: original,
                            before: None,
                            after: Some(skill_path(root, &skill)),
                            backup: Some(backup),
                        });
                        s.bindings.push(Binding {
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
            },
        )
    }
    pub(crate) fn execute_local(&self, action: &str, r: &Value) -> Result<Value> {
        if action == "import_folder" {
            return Ok(serde_json::to_value(self.import_folder(
                &files::absolute(text(r, "path")?)?,
                strings(r, "selectedPaths")?,
                flag(r, "adopt"),
                None,
            )?)?);
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
            )?)?);
        }
        if action == "diagnose" {
            let s = self.snapshot()?;
            let root = Path::new(&s.storage_root);
            let mut issues = vec![];
            for b in &s.bindings {
                if fs::read_link(&b.path).ok() != Some(binding_path(root, b)) {
                    issues.push(format!("链接异常：{}", b.path));
                } else if !Path::new(&b.path).join("SKILL.md").is_file() {
                    issues.push(format!("内容缺失：{}", b.path));
                }
            }
            for digest in s
                .skills
                .iter()
                .map(|s| s.bundle_digest.clone())
                .collect::<BTreeSet<_>>()
            {
                match files::tree_digest(&root.join("objects").join(&digest).join("tree")) {
                    Ok(actual) if actual == digest => {}
                    _ => issues.push(format!("快照已变化或丢失：{digest}")),
                }
            }
            return Ok(json!({"issues":issues}));
        }
        let state = self.transact(
            action,
            match action {
                "add_target" => "添加分发目标",
                "distribute" => "分发 Skill",
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
                        s.targets.push(Target {
                            id: id(),
                            name: text(r, "name")?.trim().into(),
                            tool: optional(r, "tool", "custom").into(),
                            scope: optional(r, "scope", "project").into(),
                            path: p.display().to_string(),
                        });
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
                        // Release removed members against actual claims, then preflight the complete new revision.
                        let old = s
                            .bindings
                            .iter()
                            .filter(|b| {
                                tids.contains(&b.target_id)
                                    && b.claims.contains(&claim)
                                    && !preset.skill_ids.contains(&b.skill_id)
                            })
                            .map(|b| b.id.clone())
                            .collect::<Vec<_>>();
                        revoke(s, root, changes, &old, &claim)?;
                        distribute(s, root, changes, &skills, &tids, &claim)?;
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
                    }
                    "remove_skill" => {
                        let sid = text(r, "skillId")?;
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
                        let mode = text(r, "mode")?;
                        if !["off", "notify", "auto"].contains(&mode) {
                            return fail("无效更新模式");
                        }
                        if source.status == "detached" && mode != "off" {
                            return fail("归集后原目录是中央库软链，请重新绑定可更新的原始来源");
                        }
                        let hours = r.get("intervalHours").and_then(Value::as_u64).unwrap_or(24);
                        if !(1..=8760).contains(&hours) {
                            return fail("更新间隔应为 1 至 8760 小时");
                        }
                        source.policy = Policy {
                            mode: mode.into(),
                            interval_hours: hours as u32,
                        };
                        source.next_check = (chrono::Utc::now()
                            + chrono::Duration::hours(hours as i64))
                        .to_rfc3339();
                    }
                    "set_follow" => {
                        let b = s
                            .bindings
                            .iter_mut()
                            .find(|b| b.id == optional(r, "bindingId", ""))
                            .ok_or_else(|| error::Error::Message("分发不存在".into()))?;
                        if flag(r, "follow") && b.claims.iter().any(|c| c != "manual") {
                            return fail("预设锁定的目标通过新预设修订更新");
                        }
                        b.follow = flag(r, "follow");
                    }
                    "settings" => {
                        if r.get("catalogSites").is_some() {
                            let sites = unique(strings(r, "catalogSites")?);
                            if sites.is_empty() || sites.len() > 8 {
                                return fail("请配置 1 至 8 个搜索站点");
                            }
                            for site in &sites {
                                if !["clawhub", "skillhub", "skills.sh"].contains(&site.as_str()) {
                                    let url = reqwest::Url::parse(site).map_err(|_| {
                                        error::Error::Message("网站地址无效".into())
                                    })?;
                                    if url.scheme() != "https"
                                        || !url.username().is_empty()
                                        || url.password().is_some()
                                        || url.query().is_some()
                                        || url.fragment().is_some()
                                    {
                                        return fail("站点需要不含凭据的 HTTPS 地址");
                                    }
                                }
                            }
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
                            || files::tree_digest(&root.join("objects").join(digest).join("tree"))?
                                != digest
                        {
                            return fail("指定历史快照不存在或损坏");
                        }
                        changes.push(Change {
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
                src.next_check = (chrono::Utc::now()
                    + chrono::Duration::hours(src.policy.interval_hours as i64))
                .to_rfc3339();
            }
            Ok(())
        });
        error::Error::Message(match result {
            Ok(_) => message,
            Err(e) => format!("{message}；状态记录失败：{e}"),
        })
    }
    pub async fn check_source(&self, source_id: &str, apply: bool) -> Result<Snapshot> {
        let baseline = self.snapshot()?;
        let source = baseline
            .sources
            .iter()
            .find(|s| s.id == source_id)
            .cloned()
            .ok_or_else(|| error::Error::Message("来源不存在".into()))?;
        if source.status == "detached" {
            return fail("该来源已归集脱离原位置，更新前请绑定原始来源");
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
        let local_view = if source.kind == "local" {
            self.local_source_view(&original_path)
                .map_err(|e| self.source_failure(source_id, e.to_string()))?
        } else {
            None
        };
        let path = local_view
            .as_ref()
            .map(|v| v.path().to_path_buf())
            .unwrap_or(original_path);
        let digest =
            files::tree_digest(&path).map_err(|e| self.source_failure(source_id, e.to_string()))?;
        let scanned = files::scan(&path.join(&source.scan_subdir))
            .map_err(|e| self.source_failure(source_id, e.to_string()))?;
        let version = if prepared.version.is_empty() {
            digest.clone()
        } else {
            prepared.version.clone()
        };
        self.transact(
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
                if src.version != source.version
                    || src.policy.mode != source.policy.mode
                    || src.path != source.path
                {
                    return fail("检查期间来源或策略已变化，请重试");
                }
                let changed = s
                    .skills
                    .iter()
                    .any(|x| x.source_id == source_id && x.bundle_digest != digest);
                let mut status = if changed { "available" } else { "current" }.to_string();
                let mut details = vec![];
                if changed && apply {
                    files::snapshot_tree(root, &path)?;
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
                    for binding in &mut s.bindings {
                        if binding.follow
                            && binding.claims.iter().all(|c| c == "manual")
                            && let Some(skill) = s.skills.iter().find(|x| {
                                x.id == binding.skill_id
                                    && x.source_id == source_id
                                    && x.bundle_digest != binding.digest
                            })
                        {
                            changes.push(Change {
                                path: PathBuf::from(&binding.path),
                                before: Some(binding_path(root, binding)),
                                after: Some(skill_path(root, skill)),
                                backup: None,
                            });
                            binding.digest = skill.bundle_digest.clone();
                            binding.version = skill.version.clone();
                        }
                    }
                    let new_count = entries
                        .keys()
                        .filter(|rel| {
                            !s.skills
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
                let src = s.sources.iter_mut().find(|x| x.id == source_id).unwrap();
                src.last_checked = now();
                src.next_check = (chrono::Utc::now()
                    + chrono::Duration::hours(src.policy.interval_hours as i64))
                .to_rfc3339();
                src.status = status;
                src.error = details.join("；");
                if apply {
                    src.version = version;
                }
                Ok(())
            },
        )
    }
    pub async fn run_due_updates(&self) -> Result<()> {
        let state = self.snapshot()?;
        if !state.initialized {
            return Ok(());
        }
        for source in state.sources {
            if source.policy.mode != "off"
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
