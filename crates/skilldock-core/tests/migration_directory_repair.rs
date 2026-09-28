use serde_json::{Value, json};
use skilldock_core::{Engine, files};
use std::{fs, path::PathBuf};

struct Fixture {
    _temp: tempfile::TempDir,
    engine: Engine,
    root: PathBuf,
    target: PathBuf,
}

impl Fixture {
    async fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let base = fs::canonicalize(temp.path()).unwrap();
        let config = base.join("config");
        let root = base.join("library");
        let engine = Engine::new(Some(config.clone())).unwrap();
        let mut state = engine.configure(root.to_str().unwrap()).unwrap();
        let internal = PathBuf::from(&state.storage_root);
        for entry in fs::read_dir(&internal).unwrap() {
            let entry = entry.unwrap();
            fs::rename(entry.path(), root.join(entry.file_name())).unwrap();
        }
        fs::remove_dir(internal).unwrap();
        state.schema_version = 2;
        state.storage_root = root.display().to_string();
        files::atomic_json(&root.join("state.json"), &state).unwrap();
        files::atomic_json(&config.join("config.json"), &json!({"storage_root":root})).unwrap();
        let source = base.join("source");
        let target = base.join("target");
        fs::create_dir_all(&target).unwrap();
        for name in ["alpha", "beta"] {
            fs::create_dir_all(source.join(name)).unwrap();
            fs::write(
                source.join(name).join("SKILL.md"),
                format!("---\nname: {name}\n---\nlibrary content"),
            )
            .unwrap();
        }
        engine.execute(json!({"action":"import_folder","path":source,"selectedPaths":[source.join("alpha"),source.join("beta")],"adopt":false})).await.unwrap();
        let state = engine
            .execute(json!({"action":"add_target","path":target,"name":"Tool"}))
            .await
            .unwrap();
        let skills: Vec<_> = state["skills"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s["id"].clone())
            .collect();
        engine.execute(json!({"action":"distribute","skillIds":skills,"targetIds":[state["targets"][0]["id"]],"expectedRevision":state["revision"]})).await.unwrap();
        for name in ["alpha", "beta"] {
            files::remove_link(&target.join(name)).unwrap();
            fs::create_dir(target.join(name)).unwrap();
            fs::write(
                target.join(name).join("SKILL.md"),
                format!("local {name} edits"),
            )
            .unwrap();
            fs::write(target.join(name).join("local-only.txt"), "preserve me").unwrap();
        }
        Self {
            _temp: temp,
            engine,
            root,
            target,
        }
    }

    async fn preview(&self) -> Value {
        self.engine
            .execute(json!({"action":"preview_single_content"}))
            .await
            .unwrap()
    }

    async fn enable(&self, preview: &Value) -> Result<Value, skilldock_core::error::Error> {
        self.engine.execute(json!({"action":"enable_single_content","expectedRevision":preview["revision"],"choices":[],"directoryRepairs":preview["directoryRepairs"]})).await
    }

    fn assert_originals(&self) {
        for name in ["alpha", "beta"] {
            assert!(!self.target.join(name).is_symlink());
            assert_eq!(
                fs::read_to_string(self.target.join(name).join("SKILL.md")).unwrap(),
                format!("local {name} edits")
            );
        }
        assert_eq!(self.engine.snapshot().unwrap().schema_version, 2);
    }
}

#[tokio::test]
async fn preview_requires_explicit_confirmation_and_preserves_real_directories() {
    let f = Fixture::new().await;
    let before = fs::read(f.root.join("state.json")).unwrap();
    let preview = f.preview().await;
    assert_eq!(preview["directoryRepairs"].as_array().unwrap().len(), 2);
    let error = f.engine.execute(json!({"action":"enable_single_content","expectedRevision":preview["revision"],"choices":[]})).await.unwrap_err().to_string();
    assert!(error.contains("实体文件夹"));
    assert!(error.contains(f.target.to_str().unwrap()));
    assert!(!error.contains("os error 22"));
    assert_eq!(fs::read(f.root.join("state.json")).unwrap(), before);
    f.assert_originals();
}

#[tokio::test]
async fn changed_directory_after_preview_is_rejected_before_any_file_changes() {
    let f = Fixture::new().await;
    let preview = f.preview().await;
    fs::write(
        f.target.join("beta/local-only.txt"),
        "changed after preview",
    )
    .unwrap();
    assert!(
        f.enable(&preview)
            .await
            .unwrap_err()
            .to_string()
            .contains("重新预览")
    );
    f.assert_originals();
    assert!(!fs::read_dir(&f.target).unwrap().any(|e| {
        e.unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".skilldock-backup-")
    }));
}

#[tokio::test]
async fn confirmed_repair_backs_up_even_unchanged_binding_and_restores_after_layout_migration() {
    let f = Fixture::new().await;
    let mut prior = f.engine.snapshot().unwrap();
    prior.bindings[0].borrowed = true;
    prior.bindings[0].original_link = Some("/previous-external-link".into());
    files::atomic_json(&f.root.join("state.json"), &prior).unwrap();
    let preview = f.preview().await;
    let state = f.enable(&preview).await.unwrap();
    assert_eq!(state["schemaVersion"], 3);
    assert_eq!(state["bindings"][0]["borrowed"], false);
    assert!(state["bindings"][0]["originalLink"].is_null());
    for name in ["alpha", "beta"] {
        assert!(f.target.join(name).is_symlink());
        assert!(
            fs::read_to_string(f.target.join(name).join("SKILL.md"))
                .unwrap()
                .contains("library content")
        );
    }
    f.engine
        .execute(json!({"action":"arrange_library"}))
        .await
        .unwrap();
    let backups = f
        .engine
        .execute(json!({"action":"list_backups"}))
        .await
        .unwrap();
    let backup = &backups.as_array().unwrap()[0];
    let state = f.engine.snapshot().unwrap();
    f.engine.execute(json!({"action":"restore_backup","backupId":backup["id"],"expectedRevision":state.revision})).await.unwrap();
    for name in ["alpha", "beta"] {
        assert!(!f.target.join(name).is_symlink());
        assert_eq!(
            fs::read_to_string(f.target.join(name).join("SKILL.md")).unwrap(),
            format!("local {name} edits")
        );
        assert_eq!(
            fs::read_to_string(f.target.join(name).join("local-only.txt")).unwrap(),
            "preserve me"
        );
    }
}

#[tokio::test]
async fn old_binding_repair_points_to_current_snapshot_and_preserves_actual_edits() {
    let f = Fixture::new().await;
    let state = f.engine.snapshot().unwrap();
    let skill = &state.skills[0];
    let tree = f
        .root
        .join("objects")
        .join(&skill.bundle_digest)
        .join("tree");
    fs::create_dir_all(f.root.join("skills")).unwrap();
    let alias = f.root.join("skills").join(&skill.name);
    let version_alias = f.root.join("skills").join(format!(
        "{}@{}",
        skill.name,
        skill.version.chars().take(8).collect::<String>()
    ));
    files::create_link(&tree.join(&skill.relative_path), &alias, true).unwrap();
    files::create_link(&tree.join(&skill.relative_path), &version_alias, true).unwrap();
    fs::write(
        tree.join(&skill.relative_path).join("new.txt"),
        "current snapshot update",
    )
    .unwrap();
    let preview = f
        .engine
        .execute(json!({"action":"preview_snapshot_refresh","skillId":skill.id}))
        .await
        .unwrap();
    f.engine.execute(json!({"action":"refresh_snapshot","skillId":skill.id,"expectedRevision":preview["revision"],"contentDigest":preview["contentDigest"]})).await.unwrap();
    let preview = f.preview().await;
    let result = f.enable(&preview).await.unwrap();
    let current = result["skills"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["id"] == skill.id)
        .unwrap();
    assert_eq!(
        fs::read_link(&alias).unwrap(),
        f.root
            .join("objects")
            .join(current["bundleDigest"].as_str().unwrap())
            .join("tree")
            .join(&skill.relative_path)
    );
    assert!(!files::exists(&version_alias));
    for binding in result["bindings"].as_array().unwrap() {
        let skill = result["skills"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["id"] == binding["skillId"])
            .unwrap();
        assert_eq!(binding["digest"], skill["bundleDigest"]);
        assert_eq!(
            fs::read_link(binding["path"].as_str().unwrap()).unwrap(),
            f.root
                .join("objects")
                .join(skill["bundleDigest"].as_str().unwrap())
                .join("tree")
                .join(skill["relativePath"].as_str().unwrap())
        );
    }
    let journals = fs::read_dir(f.root.join("transactions"))
        .unwrap()
        .map(|e| serde_json::from_slice::<Value>(&fs::read(e.unwrap().path()).unwrap()).unwrap())
        .collect::<Vec<_>>();
    let journal = journals
        .iter()
        .find(|j| {
            j["changes"]
                .as_array()
                .unwrap()
                .iter()
                .any(|c| c["backup"].is_string())
        })
        .unwrap();
    for change in journal["changes"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["backup"].is_string())
    {
        assert!(
            PathBuf::from(change["backup"].as_str().unwrap())
                .join("local-only.txt")
                .is_file()
        );
    }
    let archived_content = files::manifest_digest(&tree).unwrap();
    let arranged = f
        .engine
        .execute(json!({"action":"arrange_library"}))
        .await
        .unwrap();
    let archive = PathBuf::from(arranged["storageRoot"].as_str().unwrap())
        .join("objects")
        .join(&skill.bundle_digest)
        .join("tree");
    assert_eq!(files::manifest_digest(&archive).unwrap(), archived_content);
}

#[tokio::test]
async fn refreshed_alias_without_old_bindings_is_recognized_from_committed_history() {
    let f = Fixture::new().await;
    let state = f.engine.snapshot().unwrap();
    let skill = &state.skills[0];
    let tree = f
        .root
        .join("objects")
        .join(&skill.bundle_digest)
        .join("tree");
    fs::create_dir_all(f.root.join("skills")).unwrap();
    let alias = f.root.join("skills").join(&skill.name);
    files::create_link(&tree.join(&skill.relative_path), &alias, true).unwrap();
    fs::write(
        tree.join(&skill.relative_path).join("added.txt"),
        "local change",
    )
    .unwrap();
    let preview = f
        .engine
        .execute(json!({"action":"preview_snapshot_refresh","skillId":skill.id}))
        .await
        .unwrap();
    f.engine.execute(json!({"action":"refresh_snapshot","skillId":skill.id,"expectedRevision":preview["revision"],"contentDigest":preview["contentDigest"]})).await.unwrap();
    let mut current = f.engine.snapshot().unwrap();
    current.bindings.clear();
    files::atomic_json(&f.root.join("state.json"), &current).unwrap();
    f.enable(&f.preview().await).await.unwrap();
    assert_eq!(
        fs::read_to_string(alias.join("added.txt")).unwrap(),
        "local change"
    );
    assert_ne!(
        fs::read_link(alias).unwrap(),
        tree.join(&skill.relative_path)
    );
}

#[tokio::test]
async fn active_snapshot_cannot_be_migrated_as_a_retired_archive() {
    let f = Fixture::new().await;
    f.enable(&f.preview().await).await.unwrap();
    let state = f.engine.snapshot().unwrap();
    let skill = &state.skills[0];
    let member = f
        .root
        .join("objects")
        .join(&skill.bundle_digest)
        .join("tree")
        .join(&skill.relative_path);
    fs::write(
        member.join("changed-after-migration.txt"),
        "must not accept as current",
    )
    .unwrap();
    let links: Vec<_> = state
        .bindings
        .iter()
        .map(|b| (b.path.clone(), fs::read_link(&b.path).unwrap()))
        .collect();
    let error = f
        .engine
        .execute(json!({"action":"arrange_library"}))
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("迁移内容校验失败"));
    assert!(error.contains(&skill.bundle_digest));
    assert_eq!(
        f.engine.snapshot().unwrap().storage_root,
        state.storage_root
    );
    assert!(member.join("changed-after-migration.txt").is_file());
    for (path, expected) in links {
        assert_eq!(fs::read_link(path).unwrap(), expected);
    }
}

#[tokio::test]
async fn snapshot_preview_groups_issues_and_refresh_unblocks_migration() {
    let f = Fixture::new().await;
    let state = f.engine.snapshot().unwrap();
    let skill = &state.skills[0];
    let tree = f
        .root
        .join("objects")
        .join(&skill.bundle_digest)
        .join("tree");
    fs::write(tree.join("new-report.txt"), "preserve current report").unwrap();
    let before = fs::read(f.root.join("state.json")).unwrap();
    let preview = f.preview().await;
    assert_eq!(fs::read(f.root.join("state.json")).unwrap(), before);
    let issues = preview["snapshotIssues"].as_array().unwrap();
    assert_eq!(issues.len(), 1, "validate a shared bundle only once");
    let issue = &issues[0];
    assert_eq!(issue["bundleDigest"], skill.bundle_digest);
    assert_eq!(issue["skillIds"].as_array().unwrap().len(), 2);
    assert_eq!(issue["sourceIds"].as_array().unwrap().len(), 1);
    assert!(issue["contentDigest"].is_string());
    let error = f.enable(&preview).await.unwrap_err().to_string();
    assert!(
        state
            .skills
            .iter()
            .any(|member| error.contains(&member.name)),
        "{error}"
    );
    assert!(error.contains(&skill.bundle_digest));
    assert!(error.contains("重新收录"));
    f.assert_originals();
    f.engine.execute(json!({"action":"refresh_snapshot","skillId":issue["skillId"],"expectedRevision":preview["revision"],"contentDigest":issue["contentDigest"]})).await.unwrap();
    let next = f.preview().await;
    assert!(next["snapshotIssues"].as_array().unwrap().is_empty());
    assert_ne!(preview["revision"], next["revision"]);
    f.enable(&next).await.unwrap();
    assert_eq!(
        fs::read_to_string(tree.join("new-report.txt")).unwrap(),
        "preserve current report"
    );
}

#[tokio::test]
async fn snapshot_with_missing_entry_is_reported_without_refresh_action() {
    let f = Fixture::new().await;
    let state = f.engine.snapshot().unwrap();
    let skill = &state.skills[0];
    fs::remove_file(
        f.root
            .join("objects")
            .join(&skill.bundle_digest)
            .join("tree")
            .join(&skill.relative_path)
            .join("SKILL.md"),
    )
    .unwrap();
    let preview = f.preview().await;
    let issue = &preview["snapshotIssues"][0];
    assert!(issue["contentDigest"].is_null());
    assert!(issue["reason"].as_str().unwrap().contains("入口文件缺失"));
    assert!(issue["reason"].as_str().unwrap().contains(&skill.name));
    f.assert_originals();
}

#[tokio::test]
async fn migration_checks_unbound_selected_snapshots_even_without_file_changes() {
    let f = Fixture::new().await;
    let mut state = f.engine.snapshot().unwrap();
    state.bindings.clear();
    files::atomic_json(&f.root.join("state.json"), &state).unwrap();
    let skill = &state.skills[0];
    fs::write(
        f.root
            .join("objects")
            .join(&skill.bundle_digest)
            .join("tree/changed.txt"),
        "changed",
    )
    .unwrap();
    let before = fs::read(f.root.join("state.json")).unwrap();
    let preview = f.preview().await;
    let error = f.enable(&preview).await.unwrap_err().to_string();
    assert!(error.contains(&skill.name));
    assert!(error.contains(&skill.bundle_digest));
    assert_eq!(fs::read(f.root.join("state.json")).unwrap(), before);
}

#[tokio::test]
async fn library_collision_does_not_adopt_wrong_member_or_real_directory() {
    for real_directory in [false, true] {
        let f = Fixture::new().await;
        let state = f.engine.snapshot().unwrap();
        let alpha = state.skills.iter().find(|s| s.name == "alpha").unwrap();
        let beta = state.skills.iter().find(|s| s.name == "beta").unwrap();
        fs::create_dir_all(f.root.join("skills")).unwrap();
        let alias = f.root.join("skills/alpha");
        let wrong_member = f
            .root
            .join("objects")
            .join(&alpha.bundle_digest)
            .join("tree")
            .join(&beta.relative_path);
        if real_directory {
            fs::create_dir(&alias).unwrap();
            fs::write(alias.join("keep.txt"), "foreign directory").unwrap();
        } else {
            files::create_link(&wrong_member, &alias, true).unwrap();
        }
        let error = f.enable(&f.preview().await).await.unwrap_err().to_string();
        assert!(error.contains("目录已被占用"));
        if real_directory {
            assert_eq!(
                fs::read_to_string(alias.join("keep.txt")).unwrap(),
                "foreign directory"
            );
        } else {
            assert_eq!(fs::read_link(alias).unwrap(), wrong_member);
        }
        f.assert_originals();
    }
}

#[tokio::test]
async fn ordinary_file_collision_reports_path_without_overwriting() {
    let f = Fixture::new().await;
    fs::remove_dir_all(f.target.join("alpha")).unwrap();
    fs::write(f.target.join("alpha"), "not a skill directory").unwrap();
    let error = f
        .engine
        .execute(json!({"action":"preview_single_content"}))
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("普通文件"));
    assert!(error.contains(f.target.join("alpha").to_str().unwrap()));
    assert_eq!(
        fs::read_to_string(f.target.join("alpha")).unwrap(),
        "not a skill directory"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn transaction_failure_restores_already_backed_up_directories() {
    let f = Fixture::new().await;
    // The second binding's parent becomes an alias. Planning can inspect it,
    // but apply must refuse it after the first directory has been backed up.
    let other = f.target.with_file_name("other");
    let actual = f.target.with_file_name("actual");
    fs::create_dir(&actual).unwrap();
    fs::rename(f.target.join("beta"), actual.join("beta")).unwrap();
    std::os::unix::fs::symlink(&actual, &other).unwrap();
    let mut state = f.engine.snapshot().unwrap();
    state.bindings.sort_by_key(|b| b.path.clone());
    state.bindings[1].path = other.join("beta").display().to_string();
    files::atomic_json(&f.root.join("state.json"), &state).unwrap();
    let preview = f.preview().await;
    assert!(
        f.enable(&preview)
            .await
            .unwrap_err()
            .to_string()
            .contains("父目录发生变化")
    );
    assert!(!f.target.join("alpha").is_symlink());
    assert_eq!(
        fs::read_to_string(f.target.join("alpha/SKILL.md")).unwrap(),
        "local alpha edits"
    );
    assert_eq!(
        fs::read_to_string(actual.join("beta/SKILL.md")).unwrap(),
        "local beta edits"
    );
    let state = f.engine.snapshot().unwrap();
    assert_eq!(state.schema_version, 2);
    assert_eq!(state.tasks.last().unwrap().status, "failed");
}

#[tokio::test]
async fn migration_preserves_indirect_links_through_managed_bindings_in_retained_targets() {
    let f = Fixture::new().await;
    f.enable(&f.preview().await).await.unwrap();
    let mut state = f.engine.snapshot().unwrap();
    // A removed and re-added target remains in the conservative scan roots.
    state
        .unmanaged_target_paths
        .push(f.target.display().to_string());
    let alias_root = f.root.parent().unwrap().join("retained");
    fs::create_dir(&alias_root).unwrap();
    state
        .unmanaged_target_paths
        .push(alias_root.display().to_string());
    let alpha = f.target.join("alpha");
    let expected = fs::read_link(&alpha).unwrap();
    let relative =
        PathBuf::from("..").join(expected.strip_prefix(f.root.parent().unwrap()).unwrap());
    files::remove_link(&alpha).unwrap();
    files::create_link(&relative, &alpha, true).unwrap();
    let first = alias_root.join("first");
    let second = alias_root.join("second");
    files::create_link(&PathBuf::from("../target/alpha"), &first, true).unwrap();
    files::create_link(&PathBuf::from("first"), &second, true).unwrap();
    files::atomic_json(&f.root.join("state.json"), &state).unwrap();
    let content = fs::read(alpha.join("SKILL.md")).unwrap();
    let migrated = f
        .engine
        .execute(json!({"action":"arrange_library"}))
        .await
        .unwrap();
    assert!(
        migrated["storageRoot"]
            .as_str()
            .unwrap()
            .ends_with("/.skilldock")
    );
    assert!(!f.root.join("objects").exists());
    assert_eq!(
        fs::read_link(first).unwrap(),
        PathBuf::from("../target/alpha")
    );
    assert_eq!(fs::read_link(&second).unwrap(), PathBuf::from("first"));
    assert_eq!(fs::read(second.join("SKILL.md")).unwrap(), content);
    assert_eq!(
        fs::canonicalize(&alpha).unwrap(),
        fs::canonicalize(&second).unwrap()
    );
}

#[tokio::test]
async fn migration_still_blocks_direct_unmanaged_links_beside_managed_links() {
    let f = Fixture::new().await;
    f.enable(&f.preview().await).await.unwrap();
    let mut state = f.engine.snapshot().unwrap();
    state
        .unmanaged_target_paths
        .push(f.target.display().to_string());
    let direct = f.target.join("unregistered-copy");
    let destination = fs::read_link(f.target.join("alpha")).unwrap();
    files::create_link(&destination, &direct, true).unwrap();
    files::atomic_json(&f.root.join("state.json"), &state).unwrap();
    let before = fs::read(f.root.join("state.json")).unwrap();
    let error = f
        .engine
        .execute(json!({"action":"arrange_library"}))
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains(direct.to_str().unwrap()), "{error}");
    assert!(error.contains("未随迁移调整"), "{error}");
    assert_eq!(fs::read(f.root.join("state.json")).unwrap(), before);
    assert_eq!(fs::read_link(direct).unwrap(), destination);
    assert!(!f.root.join(".skilldock").exists());
}

#[tokio::test]
async fn migration_rejects_changed_managed_binding_even_when_indirect_alias_uses_it() {
    let f = Fixture::new().await;
    f.enable(&f.preview().await).await.unwrap();
    let mut state = f.engine.snapshot().unwrap();
    state
        .unmanaged_target_paths
        .push(f.target.display().to_string());
    let alpha = f.target.join("alpha");
    let wrong = fs::read_link(f.target.join("beta")).unwrap();
    files::remove_link(&alpha).unwrap();
    files::create_link(&wrong, &alpha, true).unwrap();
    files::create_link(&PathBuf::from("alpha"), &f.target.join("alias"), true).unwrap();
    files::atomic_json(&f.root.join("state.json"), &state).unwrap();
    let before = fs::read(f.root.join("state.json")).unwrap();
    let error = f
        .engine
        .execute(json!({"action":"arrange_library"}))
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("未随迁移调整"), "{error}");
    assert_eq!(fs::read(f.root.join("state.json")).unwrap(), before);
    assert_eq!(fs::read_link(alpha).unwrap(), wrong);
}
