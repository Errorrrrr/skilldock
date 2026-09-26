use clap::{Args, ValueEnum};
use serde_json::{Value, json};
use skilldock_core::{
    Engine,
    error::{Result, fail},
    files,
    model::{Snapshot, Target},
};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Debug, PartialEq, ValueEnum)]
pub enum SourceKind {
    Auto,
    Local,
    Git,
    Catalog,
}

#[derive(Args, Debug)]
pub struct InstallArgs {
    /// 网站标识、Git URL 或本地目录（相对路径以 ./ 开头）
    pub source: String,
    #[arg(long, value_enum, default_value = "auto")]
    pub from: SourceKind,
    /// 工具标识（用户级）或已登记的目标 ID；可重复指定
    #[arg(long = "to")]
    pub targets: Vec<String>,
    #[arg(long)]
    pub site: Option<String>,
    #[arg(long)]
    pub reference: Option<String>,
    #[arg(long)]
    pub subdir: Option<String>,
    /// 本地目录或 Git 包中的成员相对路径；省略时收录全部成员
    #[arg(long = "select")]
    pub selected: Vec<String>,
}

fn local_path(input: &str) -> Result<PathBuf> {
    let path = if input.starts_with('~') {
        files::absolute(input)?
    } else {
        PathBuf::from(input)
    };
    Ok(fs::canonicalize(path)?)
}

fn kind(input: &InstallArgs) -> Result<SourceKind> {
    let kind = if input.from != SourceKind::Auto {
        input.from
    } else if input.site.is_some() {
        SourceKind::Catalog
    } else if input.source.starts_with("https://")
        || input.source.starts_with("http://")
        || input.source.starts_with("ssh://")
        || input
            .source
            .split_once(':')
            .is_some_and(|(host, _)| host.contains('@'))
        || input.source.starts_with("file://")
    {
        SourceKind::Git
    } else if input.source.starts_with('.')
        || input.source.starts_with('~')
        || Path::new(&input.source).is_absolute()
    {
        SourceKind::Local
    } else {
        SourceKind::Catalog
    };
    if kind != SourceKind::Git && (input.reference.is_some() || input.subdir.is_some()) {
        return fail("--reference 和 --subdir 仅用于 Git 来源");
    }
    if kind != SourceKind::Catalog && input.site.is_some() {
        return fail("--site 仅用于网站来源");
    }
    if kind == SourceKind::Catalog && !input.selected.is_empty() {
        return fail("网站安装不支持 --select，请使用 Git 或本地来源选择成员");
    }
    Ok(kind)
}

// Resolve every requested destination before downloading or importing anything.
// Tool aliases address user-level directories only; project targets use their explicit ID.
fn destinations(state: &Snapshot, requested: &[String]) -> Result<Vec<Target>> {
    let mut result = vec![];
    for selector in requested {
        let target = if let Some(target) = state.targets.iter().find(|t| &t.id == selector) {
            target.clone()
        } else {
            let matches: Vec<_> = state
                .targets
                .iter()
                .filter(|t| &t.tool == selector && t.scope == "user")
                .collect();
            if matches.len() > 1 {
                return fail(format!(
                    "{selector} 有多个用户级目标，请通过 skilldock list --json 选择具体目标 ID"
                ));
            }
            if let Some(target) = matches.first() {
                (*target).clone()
            } else {
                let profile = state
                    .settings
                    .agent_profiles
                    .iter()
                    .find(|p| &p.id == selector)
                    .ok_or_else(|| {
                        skilldock_core::error::Error::Message(format!("未知工具或目标：{selector}"))
                    })?;
                let paths: Vec<_> = profile
                    .user_paths
                    .iter()
                    .map(|p| files::absolute(p))
                    .collect::<Result<_>>()?;
                let existing: Vec<_> = paths.iter().filter(|p| files::exists(p)).collect();
                let physical: BTreeSet<_> = existing
                    .iter()
                    .map(|p| fs::canonicalize(p))
                    .collect::<std::io::Result<_>>()?;
                if physical.len() > 1 {
                    return fail(format!(
                        "{selector} 存在多个目录，请先用 skilldock target 登记所需目录，再传目标 ID"
                    ));
                }
                let path = if let Some(path) = physical.first() {
                    path.clone()
                } else {
                    paths.first().cloned().ok_or_else(|| {
                        skilldock_core::error::Error::Message(format!("{selector} 未配置用户目录"))
                    })?
                };
                if let Some(target) = state.targets.iter().find(|t| Path::new(&t.path) == path) {
                    target.clone()
                } else {
                    Target {
                        id: String::new(),
                        name: format!("{} · 用户级", profile.name),
                        tool: selector.clone(),
                        scope: "user".into(),
                        path: path.display().to_string(),
                    }
                }
            }
        };
        if !result.iter().any(|t: &Target| t.path == target.path) {
            result.push(target);
        }
    }
    Ok(result)
}

fn catalog_ids(state: &Snapshot, slug: &str, site: &str) -> Result<Vec<String>> {
    let canonical = skilldock_core::network::canonical_catalog_site(site)?;
    let sources: BTreeSet<_> = state
        .sources
        .iter()
        .filter(|s| {
            matches!(s.kind.as_str(), "catalog" | "clawhub")
                && s.reference == slug.trim()
                && skilldock_core::network::canonical_catalog_site(&s.url)
                    .is_ok_and(|site| site == canonical)
        })
        .map(|s| &s.id)
        .collect();
    Ok(state
        .skills
        .iter()
        .filter(|s| {
            sources.contains(&s.source_id)
                || state
                    .skill_origins
                    .get(&s.id)
                    .is_some_and(|ids| ids.iter().any(|id| sources.contains(id)))
        })
        .map(|s| s.id.clone())
        .collect())
}

async fn import(engine: &Engine, input: &InstallArgs, kind: SourceKind) -> Result<Vec<String>> {
    match kind {
        SourceKind::Local => {
            let path = local_path(&input.source)?;
            let preview = engine
                .execute(json!({"action":"preview_local_source","path":path}))
                .await?;
            let items = preview["items"]
                .as_array()
                .ok_or_else(|| skilldock_core::error::Error::Message("来源预览格式无效".into()))?;
            let selected: Vec<String> = if input.selected.is_empty() {
                items
                    .iter()
                    .filter_map(|i| i["path"].as_str().map(str::to_string))
                    .collect()
            } else {
                input
                    .selected
                    .iter()
                    .map(|relative| {
                        files::safe_relative(relative)?;
                        let member = local_path(path.join(relative).to_str().unwrap_or(""))?;
                        if !member.starts_with(&path) {
                            return fail("成员不在来源目录内");
                        }
                        Ok(member.display().to_string())
                    })
                    .collect::<Result<_>>()?
            };
            if selected.is_empty() {
                return fail("来源目录中没有可安装的 Skill");
            }
            let wanted: BTreeSet<_> = selected
                .iter()
                .map(|p| {
                    Path::new(p)
                        .strip_prefix(&path)
                        .map(|p| p.to_string_lossy().replace('\\', "/"))
                })
                .collect::<std::result::Result<_, _>>()
                .map_err(|_| skilldock_core::error::Error::Message("成员不在来源目录内".into()))?;
            let current = engine.snapshot()?;
            let existing = current.sources.iter().find(|s| {
                s.kind == "local_managed"
                    && Path::new(&s.path) == path
                    && s.updates_removed != Some(true)
            });
            let mut selected: BTreeSet<_> = selected.into_iter().collect();
            // Installing another member is additive. Do not revoke previous source
            // claims merely because this invocation selected a different member.
            if let Some(ids) = existing.and_then(|s| s.local_member_ids.as_ref()) {
                for skill in current.skills.iter().filter(|s| ids.contains(&s.id)) {
                    selected.insert(path.join(&skill.relative_path).display().to_string());
                }
            }
            let result = engine.execute(json!({"action":"save_local_source","path":path,
                "name":existing.map(|s| s.name.as_str()).unwrap_or_else(|| path.file_name().and_then(|s| s.to_str()).unwrap_or("CLI 来源")),
                "selectedPaths":selected,"revision":preview["revision"],"contentDigest":preview["contentDigest"]})).await?;
            let state: Snapshot = serde_json::from_value(result["snapshot"].clone())?;
            let members = state
                .sources
                .iter()
                .find(|s| s.id == result["sourceId"].as_str().unwrap_or(""))
                .and_then(|s| s.local_member_ids.clone())
                .unwrap_or_default();
            Ok(state
                .skills
                .iter()
                .filter(|s| members.contains(&s.id) && wanted.contains(&s.relative_path))
                .map(|s| s.id.clone())
                .collect())
        }
        SourceKind::Git => {
            let preview = engine.execute(json!({"action":"preview_git_package","url":input.source,
                "reference":input.reference.as_deref().unwrap_or("HEAD"),"subdir":input.subdir.as_deref().unwrap_or("")})).await?;
            let mut request = json!({"action":"import_git_package","token":preview["token"],"revision":preview["revision"]});
            if !input.selected.is_empty() {
                request["selectedPaths"] = json!(input.selected);
            }
            let result = engine.execute(request).await?;
            Ok(serde_json::from_value(result["skillIds"].clone())?)
        }
        SourceKind::Catalog => {
            let site = input.site.as_deref().unwrap_or("clawhub");
            let result = engine
                .execute(json!({"action":"install_catalog","slug":input.source,"site":site}))
                .await?;
            catalog_ids(&serde_json::from_value(result)?, &input.source, site)
        }
        SourceKind::Auto => unreachable!(),
    }
}

pub async fn run(config_dir: Option<PathBuf>, input: &InstallArgs) -> Value {
    let mut report = json!({"status":"failed","source":input.source,"stage":"validate",
        "imported":false,"skillIds":[],"targets":[],"error":null});
    let outcome: Result<()> = async {
        let kind = kind(input)?;
        let engine = Engine::new(config_dir)?;
        let state = engine.snapshot()?;
        if !state.initialized { return fail("请先运行 skilldock init <统一库绝对路径>，或在桌面端设置统一目录"); }
        let targets = destinations(&state, &input.targets)?;
        report["stage"] = json!("import");
        let ids = import(&engine, input, kind).await?;
        report["imported"] = json!(true);
        report["skillIds"] = json!(ids);
        if ids.is_empty() { return fail("内容已入库，但没有解析到来源成员，请通过 skilldock list --json 检查"); }
        report["stage"] = json!("distribute");
        let mut failed = false;
        for mut target in targets {
            let mut entry = json!({"id":target.id,"tool":target.tool,"path":target.path,"status":"failed","error":null});
            let result: Result<()> = async {
                // Another CLI may have registered the same directory since validation.
                if target.id.is_empty() {
                    let state = engine.snapshot()?;
                    let path = fs::canonicalize(&target.path).unwrap_or_else(|_| PathBuf::from(&target.path));
                    if let Some(found) = state.targets.iter().find(|t| Path::new(&t.path) == path) {
                        target = found.clone();
                    } else {
                        let result = engine.execute(json!({"action":"add_target","name":target.name,
                            "path":target.path,"tool":target.tool,"scope":target.scope})).await?;
                        let state: Snapshot = serde_json::from_value(result)?;
                        let path = fs::canonicalize(&target.path)?;
                        target = state.targets.into_iter().find(|t| Path::new(&t.path) == path)
                            .ok_or_else(|| skilldock_core::error::Error::Message("目标登记后未找到目录".into()))?;
                    }
                }
                entry["id"] = json!(target.id);
                let plan = engine.execute(json!({"action":"plan","skillIds":ids,"targetIds":[target.id]})).await?;
                entry["plan"] = plan.clone();
                let errors: Vec<_> = plan["items"].as_array().into_iter().flatten()
                    .filter_map(|i| i["error"].as_str()).filter(|e| !e.is_empty()).collect();
                if !errors.is_empty() { return fail(errors.join("；")); }
                engine.execute(json!({"action":"distribute","skillIds":ids,"targetIds":[target.id],
                    "expectedRevision":plan["revision"]})).await?;
                Ok(())
            }.await;
            match result {
                Ok(()) => entry["status"] = json!("succeeded"),
                Err(error) => { failed = true; entry["error"] = json!(error.to_string()); }
            }
            report["targets"].as_array_mut().unwrap().push(entry);
        }
        if failed { return fail("已入库，但部分目标分发失败；可用返回的 skillIds 和目标 ID 重新 plan / distribute"); }
        report["stage"] = json!("complete");
        Ok(())
    }.await;
    match outcome {
        Ok(()) => report["status"] = json!("succeeded"),
        Err(error) => {
            if report["imported"] == true {
                report["status"] = json!("partial");
            }
            report["error"] = json!(error.to_string());
        }
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_resolution_handles_aliases_and_merged_origins_without_new_ids() {
        for (site, stored) in [
            ("skillhub", "https://api.skillhub.cn"),
            (
                "https://example.com/custom/api/v1/",
                "https://example.com/custom",
            ),
            ("clawhub", "https://clawhub.ai"),
            ("skills.sh", "https://skills.sh"),
        ] {
            let mut state = serde_json::to_value(Snapshot::empty("/unused".into())).unwrap();
            state["sources"] = json!([{
                "id":"catalog-source","name":"Catalog","kind":"catalog","path":"", "url":stored,
                "reference":"owner/skill","version":"1","policy":{"mode":"off","intervalHours":24},
                "lastChecked":"","nextCheck":"","status":"current","error":""
            }]);
            state["skills"] = json!([{
                "id":"canonical","name":"example","description":"test","sourceId":"original-local-source",
                "bundleDigest":"digest","relativePath":"","version":"1","installedAt":""
            }]);
            state["skillOrigins"] = json!({"canonical":["original-local-source","catalog-source"]});
            let state: Snapshot = serde_json::from_value(state).unwrap();
            assert_eq!(
                catalog_ids(&state, " owner/skill ", site).unwrap(),
                vec!["canonical"]
            );
            assert!(catalog_ids(&state, "other/skill", site).unwrap().is_empty());
            assert!(
                catalog_ids(&state, "owner/skill", "https://other.invalid")
                    .unwrap()
                    .is_empty()
            );
        }
    }
}
