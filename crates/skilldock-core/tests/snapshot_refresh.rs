use serde_json::json;
use skilldock_core::{Engine, files};
use std::fs;

#[tokio::test]
async fn refresh_preserves_old_links_and_distributes_after_finder_changes() {
    let temp = tempfile::tempdir().unwrap();
    let base = fs::canonicalize(temp.path()).unwrap();
    let engine = Engine::new(Some(base.join("config"))).unwrap();
    let root = base.join("library");
    let source = base.join("source");
    let one = base.join("one");
    let two = base.join("two");
    fs::create_dir_all(source.join("contact")).unwrap();
    fs::create_dir_all(&one).unwrap();
    fs::create_dir_all(&two).unwrap();
    fs::write(
        source.join("contact/SKILL.md"),
        "---\nname: contact\ndescription: test\n---\n# Contact",
    )
    .unwrap();
    fs::write(source.join(".DS_Store"), "old Finder settings").unwrap();
    let configured = engine.configure(root.to_str().unwrap()).unwrap();
    let root = std::path::PathBuf::from(configured.storage_root);
    let mut state = engine.execute(json!({"action":"import_folder","path":source,"selectedPaths":[source.join("contact")],"adopt":false})).await.unwrap();
    // New imports ignore Finder metadata. Construct the legacy snapshot explicitly
    // so this regression still exercises repair of an old full-tree digest.
    let legacy_digest = files::snapshot_tree(&root, &source).unwrap();
    state["schemaVersion"] = json!(2);
    state["libraryEntries"] = json!({});
    state["skills"][0]["bundleDigest"] = json!(legacy_digest);
    state["skills"][0]["version"] = json!(&legacy_digest[..12]);
    state["sources"][0]["version"] = json!(legacy_digest);
    files::atomic_json(&root.join("state.json"), &state).unwrap();
    let sid = state["skills"][0]["id"].as_str().unwrap();
    let old_digest = state["skills"][0]["bundleDigest"].as_str().unwrap();
    let old_tree = root.join("objects").join(old_digest).join("tree");
    engine
        .execute(json!({"action":"add_target","name":"one","path":one}))
        .await
        .unwrap();
    let state = engine
        .execute(json!({"action":"add_target","name":"two","path":two}))
        .await
        .unwrap();
    let tid1 = state["targets"][0]["id"].as_str().unwrap();
    let tid2 = state["targets"][1]["id"].as_str().unwrap();
    engine.execute(json!({"action":"distribute","skillIds":[sid],"targetIds":[tid1],"expectedRevision":state["revision"]})).await.unwrap();
    let old_link = fs::read_link(one.join("contact")).unwrap();
    fs::write(old_tree.join(".DS_Store"), "Finder changed preferences").unwrap();
    let state = engine.snapshot().unwrap();
    assert!(engine.execute(json!({"action":"distribute","skillIds":[sid],"targetIds":[tid2],"expectedRevision":state.revision})).await.is_err());
    let preview = engine
        .execute(json!({"action":"preview_snapshot_refresh","skillId":sid}))
        .await
        .unwrap();
    let state = engine.execute(json!({"action":"refresh_snapshot","skillId":sid,"expectedRevision":preview["revision"],"contentDigest":preview["contentDigest"]})).await.unwrap();
    assert_eq!(fs::read_link(one.join("contact")).unwrap(), old_link);
    assert!(old_tree.join("contact/SKILL.md").is_file());
    let new_tree = root
        .join("objects")
        .join(state["skills"][0]["bundleDigest"].as_str().unwrap())
        .join("tree");
    assert_ne!(new_tree, old_tree);
    fs::write(new_tree.join(".DS_Store"), "another Finder change").unwrap();
    engine.execute(json!({"action":"distribute","skillIds":[sid],"targetIds":[tid2],"expectedRevision":state["revision"]})).await.unwrap();
    assert_eq!(
        fs::read_link(two.join("contact")).unwrap(),
        new_tree.join("contact")
    );
    assert_eq!(
        fs::read_to_string(two.join("contact/SKILL.md")).unwrap(),
        fs::read_to_string(source.join("contact/SKILL.md")).unwrap()
    );
    fs::write(new_tree.join("contact/SKILL.md"), "changed skill content").unwrap();
    assert!(
        !files::snapshot_matches(
            &new_tree,
            state["skills"][0]["bundleDigest"].as_str().unwrap()
        )
        .unwrap()
    );
    assert!(engine.execute(json!({"action":"refresh_snapshot","skillId":sid,"expectedRevision":preview["revision"],"contentDigest":preview["contentDigest"]})).await.is_err());
    let changed = engine
        .execute(json!({"action":"preview_snapshot_refresh","skillId":sid}))
        .await
        .unwrap();
    fs::write(new_tree.join("contact/SKILL.md"), "changed after preview").unwrap();
    assert!(engine.execute(json!({"action":"refresh_snapshot","skillId":sid,"expectedRevision":changed["revision"],"contentDigest":changed["contentDigest"]})).await.is_err());
}

#[test]
fn finder_metadata_is_ignored_only_for_regular_files() {
    let temp = tempfile::tempdir().unwrap();
    let base = fs::canonicalize(temp.path()).unwrap();
    fs::write(base.join("SKILL.md"), "original").unwrap();
    let digest = files::content_digest(temp.path()).unwrap();
    fs::write(base.join(".DS_Store"), "metadata").unwrap();
    assert!(files::snapshot_matches(temp.path(), &digest).unwrap());
    fs::remove_file(base.join(".DS_Store")).unwrap();
    fs::create_dir(base.join(".DS_Store")).unwrap();
    fs::write(base.join(".DS_Store/script.sh"), "payload").unwrap();
    assert!(!files::snapshot_matches(temp.path(), &digest).unwrap());
}

#[tokio::test]
async fn skills_view_is_maintained_automatically() {
    let temp = tempfile::tempdir().unwrap();
    let base = fs::canonicalize(temp.path()).unwrap();
    let engine = Engine::new(Some(base.join("config"))).unwrap();
    let root = base.join("library");
    let source = base.join("source");
    fs::create_dir_all(source.join("calculator")).unwrap();
    fs::write(
        source.join("calculator/SKILL.md"),
        "---\nname: calculator\ndescription: math tool\n---\n# Calculator",
    )
    .unwrap();

    engine.configure(root.to_str().unwrap()).unwrap();
    assert!(root.join(".skilldock").is_dir());

    let state = engine
        .execute(json!({
            "action": "import_folder",
            "path": source,
            "selectedPaths": [source.join("calculator")],
            "adopt": false
        }))
        .await
        .unwrap();

    let digest = state["skills"][0]["bundleDigest"].as_str().unwrap();
    let object_dir = root.join(".skilldock/objects").join(digest);

    // Verify the current Skill is directly accessible by name.
    let skill_link = root.join("calculator");
    assert!(skill_link.exists(), "calculator symlink should exist");
    assert_eq!(
        fs::read_link(&skill_link).unwrap(),
        object_dir.join("tree/calculator")
    );
    assert!(skill_link.join("SKILL.md").is_file());
}
