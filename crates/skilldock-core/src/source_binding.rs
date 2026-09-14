use crate::*;

impl Engine {
    pub(crate) async fn bind_source(&self, request: &Value, preview: bool) -> Result<Value> {
        let baseline = self.snapshot()?;
        let source_id = text(request, "sourceId")?;
        let source = baseline
            .sources
            .iter()
            .find(|s| s.id == source_id)
            .ok_or_else(|| error::Error::Message("来源不存在".into()))?;
        if source.updates_removed == Some(true) {
            return fail("此来源已移除更新管理");
        }
        if source.kind == "local_reference" {
            return fail("本地引用直接跟随原目录，无需配置更新来源");
        }
        if source.status != "detached" {
            return fail("此来源已配置，请刷新后检查更新");
        }
        if source.kind == "local" {
            return fail(
                "归集记录不是统一更新来源，不能为整个安装目录绑定仓库；请从实际 Git 仓库或源码包导入并核对成员",
            );
        }
        let members: Vec<_> = baseline
            .skills
            .iter()
            .filter(|s| s.source_id == source_id)
            .collect();
        if members.is_empty() {
            return fail("此来源没有需要更新的 Skill");
        }
        let kind = text(request, "kind")?;
        let root = Path::new(&baseline.storage_root);
        let prepared = match kind {
            "git" => {
                network::prepare_git(
                    &root.join("cache"),
                    text(request, "url")?,
                    optional(request, "reference", "HEAD"),
                    optional(request, "subdir", ""),
                )
                .await?
            }
            "local" => {
                let path = files::absolute(text(request, "path")?)?;
                if !path.is_dir() {
                    return fail("源码目录不存在");
                }
                if path.starts_with(root)
                    || root.starts_with(&path)
                    || baseline.targets.iter().any(|t| {
                        let target =
                            fs::canonicalize(&t.path).unwrap_or_else(|_| PathBuf::from(&t.path));
                        path.starts_with(&target) || target.starts_with(&path)
                    })
                {
                    return fail("请选择独立源码目录，不能使用统一库或工具分发目录");
                }
                network::PreparedSource {
                    path,
                    kind: "local".into(),
                    name: source.name.clone(),
                    url: String::new(),
                    reference: String::new(),
                    version: String::new(),
                    scan_subdir: String::new(),
                }
            }
            _ => return fail("请选择 Git 仓库或独立本地源码目录"),
        };
        let original = fs::canonicalize(&prepared.path)?;
        let view = if kind == "git" {
            Self::remote_source_view(&original)?
        } else {
            self.local_source_view(&original, true)?
        };
        let path = view.as_ref().map(|v| v.path()).unwrap_or(&original);
        let scanned = files::scan(&path.join(&prepared.scan_subdir))?;
        for member in &members {
            let expected = path.join(files::safe_relative(&member.relative_path)?);
            if !scanned.items.iter().any(|item| {
                item.status == "ready"
                    && Path::new(&item.path) == expected
                    && item.name == member.name
            }) {
                return fail(format!(
                    "新来源未匹配 {}（目录：{}），请确认包含全部成员且目录结构一致",
                    member.name,
                    if member.relative_path.is_empty() {
                        "."
                    } else {
                        &member.relative_path
                    }
                ));
            }
        }
        let digest = files::content_digest(path)?;
        if preview {
            return Ok(json!({"revision": baseline.revision, "digest": digest,
                "memberNames": members.iter().map(|s| &s.name).collect::<Vec<_>>() }));
        }
        if request.get("expectedRevision").and_then(Value::as_u64) != Some(baseline.revision as u64)
            || text(request, "digest")? != digest
        {
            return fail("来源或内容已变化，请重新核对");
        }
        let state = self.transact("bind_source", "配置更新来源", |state, _, _| {
            if state.revision != baseline.revision {
                return fail("数据已变化，请重新核对来源");
            }
            let changed = state
                .skills
                .iter()
                .any(|s| s.source_id == source_id && s.bundle_digest != digest);
            let source = state
                .sources
                .iter_mut()
                .find(|s| s.id == source_id)
                .unwrap();
            source.kind = kind.into();
            source.path = original.display().to_string();
            source.url = prepared.url.clone();
            source.reference = prepared.reference.clone();
            source.scan_subdir = prepared.scan_subdir.clone();
            source.status = if changed { "available" } else { "current" }.into();
            source.error = String::new();
            source.last_checked = now();
            source.next_check = String::new();
            source.policy.mode = "off".into();
            Ok(())
        })?;
        Ok(serde_json::to_value(state)?)
    }
}
