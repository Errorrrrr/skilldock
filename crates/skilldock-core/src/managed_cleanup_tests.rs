use super::*;

fn setup() -> (tempfile::TempDir, Engine, PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let base = fs::canonicalize(temp.path()).unwrap();
    let original = base.join("original");
    fs::create_dir_all(original.join("alpha")).unwrap();
    fs::write(
        original.join("alpha/SKILL.md"),
        "---\nname: alpha\n---\nCurrent",
    )
    .unwrap();
    let engine = Engine::new(Some(base.join("config"))).unwrap();
    engine
        .configure(base.join("library").to_str().unwrap())
        .unwrap();
    let state = engine
        .transact("test", "isolate scan roots", |state, _, _| {
            state.settings.agent_profiles.clear();
            Ok(())
        })
        .unwrap();
    let preview = engine
        .preview_local_source(&json!({"path": original}))
        .unwrap();
    engine
        .save_local_source(&json!({
            "path": original, "name": "Managed source", "selectedPaths": [original.join("alpha")],
            "revision": state.revision, "contentDigest": preview["contentDigest"]
        }))
        .unwrap();
    (temp, engine, original)
}

#[test]
fn managed_source_paths_remain_scan_roots_after_configuration_is_removed() {
    let (_temp, engine, original) = setup();
    let prior = engine.snapshot().unwrap();
    assert!(Engine::scan_roots(&prior, &[]).contains(&original.display().to_string()));
    let old_content = tempfile::tempdir().unwrap();
    fs::write(
        old_content.path().join("SKILL.md"),
        "# Orphaned old content",
    )
    .unwrap();
    let root = Path::new(&prior.storage_root);
    let digest = files::snapshot_current_content(root, old_content.path()).unwrap();
    let object = root.join("objects").join(&digest).join("tree");
    files::create_link(&object, &original.join("keep-old"), true).unwrap();
    let preview = engine.preview_object_cleanup().unwrap();
    assert_eq!(
        preview
            .items
            .iter()
            .find(|i| i.digest == digest)
            .unwrap()
            .status,
        "referenced"
    );

    engine
        .transact("test", "remove source metadata", |state, _, _| {
            state.sources.clear();
            Ok(())
        })
        .unwrap();
    // Completed journals must retain the original scan location even though
    // no current source record owns it anymore.
    let preview = engine.preview_object_cleanup().unwrap();
    let item = preview.items.iter().find(|i| i.digest == digest).unwrap();
    assert_eq!(item.status, "referenced");
    assert!(item.reason.contains("软链引用"));
    assert!(object.join("SKILL.md").is_file());
}

#[test]
fn managed_source_prevents_migration_into_its_original_directory() {
    let (_temp, engine, original) = setup();
    let error = engine
        .migrate_storage(original.join("library").to_str().unwrap())
        .unwrap_err();
    assert!(error.to_string().contains("包或本地来源互相包含"));
    assert!(original.join("alpha/SKILL.md").is_file());
}

#[test]
fn unmanaged_link_inside_managed_source_blocks_migration_until_removed() {
    let (temp, engine, original) = setup();
    let state = engine.snapshot().unwrap();
    let current = skill_path(Path::new(&state.storage_root), &state.skills[0]);
    let external_link = original.join("manual-link");
    files::create_link(&current, &external_link, true).unwrap();
    let destination = temp.path().join("moved-library");

    let error = engine
        .migrate_storage(destination.to_str().unwrap())
        .unwrap_err();

    assert!(error.to_string().contains("保留链接仍指向当前统一库"));
    assert!(external_link.join("SKILL.md").is_file());
    files::remove_link(&external_link).unwrap();
    let moved = engine
        .migrate_storage(destination.to_str().unwrap())
        .unwrap();
    assert!(
        skill_path(Path::new(&moved.storage_root), &moved.skills[0])
            .join("SKILL.md")
            .is_file()
    );
}

#[test]
fn deleted_managed_source_does_not_block_cleanup_or_migration() {
    let (temp, engine, original) = setup();
    fs::remove_dir_all(&original).unwrap();

    let plan = engine.preview_object_cleanup().unwrap();
    assert!(plan.items.iter().all(|item| item.status != "blocked"));
    let moved = engine
        .migrate_storage(temp.path().join("moved-library").to_str().unwrap())
        .unwrap();

    assert_eq!(moved.sources[0].kind, "local_managed");
    assert!(
        skill_path(Path::new(&moved.storage_root), &moved.skills[0])
            .join("SKILL.md")
            .is_file()
    );
    assert!(!original.exists());
}
