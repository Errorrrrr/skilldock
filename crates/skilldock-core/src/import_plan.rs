use crate::*;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

impl Engine {
    pub(crate) fn preview_collection(&self, request: &Value) -> Result<Value> {
        self.collection_plan(&self.snapshot()?, request)
    }

    fn collection_plan(&self, state: &Snapshot, request: &Value) -> Result<Value> {
        if state.schema_version < 3 {
            return fail("请先在设置中切换为单份当前内容，再使用归集预览");
        }
        let mode = optional(request, "mode", "package");
        if !matches!(mode, "package" | "individual") {
            return fail("请选择保留整包结构或独立 Skill");
        }
        let mut roots = Vec::new();
        for path in strings(request, "paths")? {
            let path = files::absolute(&path)?;
            if fs::symlink_metadata(&path)?.file_type().is_symlink() || files::protected(&path) {
                return fail("请选择原始实体目录，不能扫描软链根或工具内部目录");
            }
            let path = fs::canonicalize(path)?;
            let library = Path::new(&state.storage_root);
            if path.starts_with(library) || library.starts_with(&path) {
                return fail("扫描目录不能与统一库互相包含");
            }
            roots.push(path.display().to_string());
        }
        roots.sort_by_key(|path| (path.len(), path.clone()));
        roots.dedup();
        if roots.is_empty() {
            return fail("请选择扫描目录");
        }
        let scan = files::scan_many(&roots)?;
        let ready: Vec<_> = scan.items.iter().filter(|i| i.status == "ready").collect();
        let mut digests: BTreeMap<String, String> = BTreeMap::new();
        let mut items = Vec::new();
        for item in &scan.items {
            let mut value = serde_json::to_value(item)?;
            if item.status == "ready" {
                let path = Path::new(&item.path);
                let import_root = if mode == "individual" {
                    // A parent Skill and its nested members must stay together.
                    ready
                        .iter()
                        .filter(|candidate| path.starts_with(&candidate.path))
                        .min_by_key(|candidate| candidate.path.len())
                        .map(|candidate| candidate.path.clone())
                        .unwrap()
                } else {
                    roots
                        .iter()
                        .find(|root| path.starts_with(root))
                        .unwrap()
                        .clone()
                };
                let digest = if let Some(digest) = digests.get(&import_root) {
                    digest.clone()
                } else {
                    let root = Path::new(&import_root);
                    let view = self.local_source_view(root, !flag(request, "adopt"))?;
                    let digest =
                        files::content_digest(view.as_ref().map(|v| v.path()).unwrap_or(root))?;
                    digests.insert(import_root.clone(), digest.clone());
                    digest
                };
                let relative = path
                    .strip_prefix(&import_root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                value["importRoot"] = json!(import_root);
                value["digest"] = json!(digest);
                value["relativePath"] = json!(relative);
            }
            items.push(value);
        }
        let candidates = items.clone();
        for (index, item) in items.iter_mut().enumerate() {
            if item["status"] != "ready" {
                continue;
            }
            let name = text(item, "name")?.to_lowercase();
            let digest = text(item, "digest")?;
            let relative = text(item, "relativePath")?;
            let known: Vec<_> = state
                .skills
                .iter()
                .filter(|skill| {
                    declared_skill_name(Path::new(&state.storage_root), skill).to_lowercase()
                        == name
                })
                .collect();
            let equal = known.iter().find(|skill| {
                skill.external_path.is_none()
                    && skill.bundle_digest == digest
                    && skill.relative_path == relative
            });
            let peers: Vec<_> = candidates
                .iter()
                .enumerate()
                .filter(|(i, other)| {
                    *i != index
                        && other["status"] == "ready"
                        && other["name"]
                            .as_str()
                            .is_some_and(|n| n.to_lowercase() == name)
                })
                .collect();
            if let Some(skill) = equal {
                item["status"] = json!("same");
                item["existingId"] = json!(skill.id);
                item["error"] = json!("完整内容与包内路径一致，将复用库中内容");
            } else if !known.is_empty()
                || peers.iter().any(|(_, other)| {
                    other["digest"] != item["digest"]
                        || other["relativePath"] != item["relativePath"]
                })
            {
                item["status"] = json!("conflict");
                item["error"] =
                    json!("同名内容或依赖包不同；请选择独立保留或跳过。同一工具只启用一个同名实现");
            } else if peers.iter().any(|(i, _)| *i < index) {
                item["status"] = json!("same");
                item["error"] = json!("与本次扫描的其他成员内容一致，将复用同一份内容");
            } else {
                item["status"] = json!("new");
            }
        }
        let fingerprint = format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&json!({
                "roots": roots, "mode": mode, "adopt": flag(request, "adopt"), "items": items,
            }))?)
        );
        let mut warnings = scan.warnings;
        if mode == "individual" {
            warnings.push("独立 Skill 模式只保存各成员目录；仅在确认成员不依赖扫描根的共享文件或兄弟目录时使用。含共享资源的工作流请选择保留整包结构。".into());
        }
        Ok(
            json!({ "revision":state.revision, "fingerprint":fingerprint,
            "mode":mode, "paths":roots, "items":items, "warnings":warnings }),
        )
    }

    pub(crate) fn collect_skills(&self, request: &Value) -> Result<Value> {
        let selected: BTreeSet<_> = strings(request, "selectedPaths")?.into_iter().collect();
        if selected.is_empty() {
            return fail("请至少选择一个 Skill");
        }
        let state = self.transact("import", "归集 Skill 到统一目录", |state, root, changes| {
            if request["expectedRevision"].as_u64() != Some(state.revision as u64) {
                return fail("资料库已变化，请重新扫描并确认");
            }
            let preview = self.collection_plan(state, request)?;
            if preview["fingerprint"] != request["fingerprint"] {
                return fail("来源内容或扫描范围已变化，请重新扫描并确认");
            }
            let items: Vec<_> = preview["items"].as_array().unwrap().iter()
                .filter(|item| item["path"].as_str().is_some_and(|p| selected.contains(p)))
                .collect();
            if items.len() != selected.len() || items.iter().any(|item| {
                !matches!(item["status"].as_str(), Some("new" | "same" | "conflict"))
            }) {
                return fail("所选条目不可归集，请重新扫描");
            }
            if flag(request, "adopt") && items.iter().any(|parent| {
                let parent_path = Path::new(parent["path"].as_str().unwrap());
                preview["items"].as_array().unwrap().iter().any(|child| {
                    let child_path = child["path"].as_str().unwrap_or_default();
                    child_path != parent_path.to_string_lossy()
                        && Path::new(child_path).starts_with(parent_path)
                        && matches!(child["status"].as_str(), Some("new" | "same" | "conflict"))
                        && !selected.contains(child_path)
                })
            }) {
                return fail("选中的父 Skill 包含未选择的嵌套成员；请一并审阅选择，或返回改为仅复制入库，避免连带接管跳过的成员");
            }
            let mut groups: BTreeMap<String, Vec<String>> = BTreeMap::new();
            for item in items {
                let path = text(item, "path")?;
                if item["status"] == "conflict" && request["resolutions"][path] != "keep" {
                    return fail("同名冲突尚未确认，请选择独立保留或跳过");
                }
                groups.entry(text(item, "importRoot")?.into()).or_default().push(path.into());
            }
            for (path, members) in groups {
                self.prepare_import(Path::new(&path), members, flag(request, "adopt"), None,
                    state, root, changes)?;
                // Confirm the just-copied object is the exact content reviewed.
                let expected_digest = preview["items"].as_array().unwrap().iter()
                    .find(|i| i["importRoot"] == path).unwrap()["digest"].as_str().unwrap();
                if state.skills.iter().filter(|skill| {
                    state.sources.iter().any(|source| source.id == skill.source_id && source.path == path)
                }).any(|skill| skill.bundle_digest != expected_digest) {
                    return fail("复制期间来源内容发生变化，请重新扫描");
                }
            }
            for (index, change) in changes.iter().enumerate() {
                if changes[..index].iter().any(|other| {
                    change.path.starts_with(&other.path) || other.path.starts_with(&change.path)
                }) {
                    return fail("归集包含交叉的原目录，请重新扫描");
                }
            }
            Ok(())
        })?;
        Ok(serde_json::to_value(state)?)
    }
}
