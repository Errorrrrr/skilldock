use serde_json::{Value, json};
use skilldock_core::Engine;
use std::{
    fs,
    path::{Path, PathBuf},
};

async fn previewed(engine: &Engine, mut request: Value) -> Value {
    let preview = engine.execute(json!({"action":"preview_local_source","path":request["path"],"sourceId":request["sourceId"]})).await.unwrap();
    request["contentDigest"] = preview["contentDigest"].clone();
    request
}

async fn run(engine: &Engine, mut request: Value) -> Value {
    if request["action"] == "save_local_source" && request.get("contentDigest").is_none() {
        request = previewed(engine, request).await;
    }
    engine.execute(request).await.unwrap()
}

fn write_skill(path: &Path, name: &str, content: &str) {
    fs::create_dir_all(path).unwrap();
    fs::write(
        path.join("SKILL.md"),
        format!("---\nname: {name}\ndescription: test\n---\n{content}"),
    )
    .unwrap();
}

fn link(from: &Path, to: &Path) {
    #[cfg(unix)]
    std::os::unix::fs::symlink(from, to).unwrap();
    #[cfg(windows)]
    std::os::windows::fs::symlink_dir(from, to).unwrap();
}

struct Fixture {
    _temp: tempfile::TempDir,
    base: PathBuf,
    engine: Engine,
    source: PathBuf,
    alpha: PathBuf,
    target: PathBuf,
    tid: Value,
}
impl Fixture {
    async fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let base = fs::canonicalize(temp.path()).unwrap();
        let source = base.join("source");
        let alpha = source.join("alpha");
        let target = base.join("target");
        write_skill(&alpha, "alpha", "original");
        fs::create_dir_all(source.join("shared")).unwrap();
        fs::write(source.join("shared/helper.txt"), "shared-original").unwrap();
        fs::create_dir(&target).unwrap();
        let engine = Engine::new(Some(base.join("config"))).unwrap();
        engine
            .configure(base.join("library").to_str().unwrap())
            .unwrap();
        let state = run(
            &engine,
            json!({"action":"add_target","path":target,"name":"Tool"}),
        )
        .await;
        let tid = state["targets"][0]["id"].clone();
        Self {
            _temp: temp,
            base,
            engine,
            source,
            alpha,
            target,
            tid,
        }
    }
    async fn save(&self) -> Value {
        run(&self.engine, json!({"action":"save_local_source","name":"Local","path":self.source,"selectedPaths":[self.alpha],"revision":self.engine.snapshot().unwrap().revision})).await
    }
    async fn apply(&self, sid: &Value) -> Value {
        run(&self.engine, json!({"action":"apply_local_source","sourceId":sid,"targetIds":[self.tid],"expectedRevision":self.engine.snapshot().unwrap().revision})).await
    }
    fn store(&self, state: &Value) {
        skilldock_core::files::atomic_json(&self.base.join("library/.skilldock/state.json"), state)
            .unwrap();
    }
    async fn legacy(&self) -> Value {
        let mut saved = self.save().await;
        let state = &mut saved["snapshot"];
        state["sources"][0]["kind"] = json!("local_reference");
        state["sources"][0]["status"] = json!("local_reference");
        state["skills"][0]["externalPath"] = json!(self.alpha);
        state["skills"][0]["bundleDigest"] = json!("");
        state["skills"][0]["version"] = json!("跟随本地内容");
        let entry = self.base.join("library/alpha");
        fs::remove_file(&entry).unwrap();
        link(&self.alpha, &entry);
        self.store(state);
        saved
    }
}

#[tokio::test]
async fn managed_source_is_independent_and_preserves_complete_package() {
    let f = Fixture::new().await;
    let saved = f.save().await;
    let sid = saved["sourceId"].clone();
    let skill = &saved["snapshot"]["skills"][0];
    assert!(skill["externalPath"].is_null());
    assert_eq!(saved["snapshot"]["sources"][0]["kind"], "local_managed");
    assert!(!skill["bundleDigest"].as_str().unwrap().is_empty());
    assert!(f.alpha.join("SKILL.md").is_file());
    assert_eq!(
        fs::read_to_string(f.source.join("shared/helper.txt")).unwrap(),
        "shared-original"
    );

    let duplicate = f.save().await;
    assert_eq!(duplicate["sourceId"], sid);
    assert_eq!(duplicate["snapshot"]["skills"].as_array().unwrap().len(), 1);
    assert_eq!(duplicate["snapshot"]["skills"][0]["id"], skill["id"]);
    let stale = previewed(&f.engine, json!({"action":"save_local_source","name":"Local","path":f.source,"selectedPaths":[f.alpha],"revision":saved["snapshot"]["revision"]})).await;
    assert!(
        f.engine
            .execute(stale)
            .await
            .unwrap_err()
            .to_string()
            .contains("资料库已变化")
    );

    fs::remove_dir_all(&f.source).unwrap();
    f.apply(&sid).await;
    let entity = fs::canonicalize(f.target.join("alpha")).unwrap();
    assert!(entity.starts_with(f.base.join("library/.skilldock")));
    assert!(
        fs::read_to_string(entity.join("SKILL.md"))
            .unwrap()
            .ends_with("original")
    );
    assert_eq!(
        fs::read_to_string(entity.parent().unwrap().join("shared/helper.txt")).unwrap(),
        "shared-original"
    );
}

#[tokio::test]
async fn original_changes_require_manual_sync_and_keep_other_claims() {
    let f = Fixture::new().await;
    let saved = f.save().await;
    let sid = saved["sourceId"].clone();
    let skill_id = saved["snapshot"]["skills"][0]["id"].clone();
    let state = f.apply(&sid).await;
    run(&f.engine, json!({"action":"distribute","skillIds":[skill_id],"targetIds":[f.tid],"expectedRevision":state["revision"]})).await;
    write_skill(&f.alpha, "alpha", "changed");
    fs::write(f.source.join("shared/helper.txt"), "shared-changed").unwrap();
    assert!(
        fs::read_to_string(f.target.join("alpha/SKILL.md"))
            .unwrap()
            .ends_with("original")
    );
    let beta = f.source.join("beta");
    write_skill(&beta, "beta", "second member");
    let synced = run(&f.engine, json!({"action":"save_local_source","sourceId":sid,"name":"Local","path":f.source,"selectedPaths":[f.alpha,beta],"revision":f.engine.snapshot().unwrap().revision})).await;
    assert!(
        fs::read_to_string(f.target.join("alpha/SKILL.md"))
            .unwrap()
            .ends_with("changed")
    );
    assert!(f.target.join("beta").is_symlink());
    let entity = fs::canonicalize(f.target.join("alpha")).unwrap();
    assert_eq!(
        fs::read_to_string(entity.parent().unwrap().join("shared/helper.txt")).unwrap(),
        "shared-changed"
    );
    let state = run(&f.engine, json!({"action":"revoke_local_source","sourceId":sid,"targetIds":[f.tid],"expectedRevision":synced["snapshot"]["revision"]})).await;
    assert!(f.target.join("alpha").is_symlink(), "manual claim remains");
    assert!(
        !f.target.join("beta").exists(),
        "source-only claim is removed"
    );
    let before = state["skills"].clone();
    let state = run(
        &f.engine,
        json!({"action":"remove_local_source","sourceId":sid,"expectedRevision":state["revision"]}),
    )
    .await;
    assert_eq!(state["skills"], before);
    assert!(f.alpha.join("SKILL.md").is_file());
}

#[tokio::test]
async fn identical_imports_keep_source_members_valid_after_deduplication() {
    let f = Fixture::new().await;
    let first = f.save().await;
    let other = f.base.join("other");
    write_skill(&other.join("alpha"), "alpha", "original");
    fs::create_dir_all(other.join("shared")).unwrap();
    fs::write(other.join("shared/helper.txt"), "shared-original").unwrap();
    let second = run(&f.engine, json!({"action":"save_local_source","name":"Other","path":other,"selectedPaths":[other.join("alpha")],"revision":f.engine.snapshot().unwrap().revision})).await;
    assert_eq!(second["snapshot"]["skills"].as_array().unwrap().len(), 1);
    let member = &second["snapshot"]["sources"][1]["localMemberIds"][0];
    assert_eq!(member, &first["snapshot"]["skills"][0]["id"]);
    f.apply(&second["sourceId"]).await;
    assert!(f.target.join("alpha").is_symlink());
}

#[tokio::test]
async fn stale_legacy_members_do_not_block_revoke_or_remove() {
    let f = Fixture::new().await;
    let legacy = f.legacy().await;
    let sid = legacy["sourceId"].clone();
    let mut state = f.apply(&sid).await;
    state["sources"][0]["localMemberIds"]
        .as_array_mut()
        .unwrap()
        .push(json!("missing-member"));
    f.store(&state);
    let state = run(&f.engine, json!({"action":"revoke_local_source","sourceId":sid,"targetIds":[f.tid],"expectedRevision":state["revision"]})).await;
    assert!(!f.target.join("alpha").exists());
    let state = run(
        &f.engine,
        json!({"action":"remove_local_source","sourceId":sid,"expectedRevision":state["revision"]}),
    )
    .await;
    assert_eq!(state["sources"][0]["updatesRemoved"], true);
    assert!(f.alpha.join("SKILL.md").is_file());
}

#[tokio::test]
async fn legacy_migration_requires_explicit_choice_and_switches_existing_links() {
    let f = Fixture::new().await;
    let legacy = f.legacy().await;
    let sid = legacy["sourceId"].clone();
    let state = f.apply(&sid).await;
    assert_eq!(fs::canonicalize(f.target.join("alpha")).unwrap(), f.alpha);
    let mut request = previewed(&f.engine, json!({"action":"save_local_source","sourceId":sid,"name":"Local","path":f.source,"selectedPaths":[f.alpha],"revision":state["revision"]})).await;
    assert!(
        f.engine
            .execute(request.clone())
            .await
            .unwrap_err()
            .to_string()
            .contains("确认复制")
    );
    request["migrate"] = json!(true);
    let migrated = run(&f.engine, request).await;
    assert_eq!(
        migrated["snapshot"]["skills"][0]["id"],
        legacy["snapshot"]["skills"][0]["id"]
    );
    assert!(migrated["snapshot"]["skills"][0]["externalPath"].is_null());
    assert!(
        fs::canonicalize(f.target.join("alpha"))
            .unwrap()
            .starts_with(f.base.join("library"))
    );
    assert!(f.alpha.join("SKILL.md").is_file());
    fs::remove_dir_all(&f.source).unwrap();
    assert!(f.target.join("alpha/SKILL.md").is_file());
}

#[tokio::test]
async fn migration_does_not_change_another_legacy_sources_shared_member() {
    let f = Fixture::new().await;
    let legacy = f.legacy().await;
    let mut state = legacy["snapshot"].clone();
    let mut other = state["sources"][0].clone();
    other["id"] = json!("other-source");
    other["path"] = json!(f.base.join("other"));
    state["sources"].as_array_mut().unwrap().push(other);
    f.store(&state);
    let request = previewed(&f.engine, json!({"action":"save_local_source","sourceId":legacy["sourceId"],"name":"Local","path":f.source,"selectedPaths":[f.alpha],"revision":state["revision"],"migrate":true})).await;
    let error = f.engine.execute(request).await.unwrap_err();
    assert!(error.to_string().contains("其他本地引用来源共享"));
    assert_eq!(
        f.engine.snapshot().unwrap().skills[0]
            .external_path
            .as_deref(),
        Some(f.alpha.to_str().unwrap())
    );
}

#[tokio::test]
async fn name_conflicts_require_explicit_retention_but_own_updates_do_not() {
    let f = Fixture::new().await;
    f.save().await;
    write_skill(&f.alpha, "alpha", "own update");
    let own = run(
        &f.engine,
        json!({"action":"preview_local_source","path":f.source}),
    )
    .await;
    assert_eq!(own["items"][0]["sameName"], false);
    f.save().await;
    let other = f.base.join("different");
    write_skill(&other.join("alpha"), "alpha", "different source");
    let preview = run(
        &f.engine,
        json!({"action":"preview_local_source","path":other}),
    )
    .await;
    assert_eq!(preview["items"][0]["sameName"], true);
    let mut request = json!({"action":"save_local_source","name":"Other","path":other,"selectedPaths":[other.join("alpha")],"revision":preview["revision"],"contentDigest":preview["contentDigest"]});
    assert!(
        f.engine
            .execute(request.clone())
            .await
            .unwrap_err()
            .to_string()
            .contains("独立保留同名")
    );
    request["keepConflicts"] = json!(true);
    let result = run(&f.engine, request).await;
    assert_eq!(result["snapshot"]["skills"].as_array().unwrap().len(), 2);
    assert!(
        result["snapshot"]["skills"][1]["name"]
            .as_str()
            .unwrap()
            .starts_with("alpha--")
    );
    write_skill(&other.join("alpha"), "alpha", "later update");
    let preview = run(
        &f.engine,
        json!({"action":"preview_local_source","path":other,"sourceId":result["sourceId"]}),
    )
    .await;
    assert_eq!(
        preview["items"][0]["sameName"], false,
        "existing independent identity is already accepted"
    );
    run(&f.engine, json!({"action":"save_local_source","name":"Other","path":other,"sourceId":result["sourceId"],"selectedPaths":[other.join("alpha")],"revision":preview["revision"],"contentDigest":preview["contentDigest"]})).await;
}

#[tokio::test]
async fn single_skill_without_frontmatter_keeps_its_directory_name() {
    let f = Fixture::new().await;
    let single = f.base.join("plain-skill");
    fs::create_dir(&single).unwrap();
    fs::write(single.join("SKILL.md"), "Instructions without YAML").unwrap();
    let saved = run(&f.engine, json!({"action":"save_local_source","name":"Single","path":single,"selectedPaths":[single],"revision":f.engine.snapshot().unwrap().revision})).await;
    assert_eq!(saved["snapshot"]["skills"][0]["name"], "plain-skill");
    f.apply(&saved["sourceId"]).await;
    assert!(f.target.join("plain-skill/SKILL.md").is_file());
    fs::write(single.join("SKILL.md"), "Updated instructions").unwrap();
    let saved = run(&f.engine, json!({"action":"save_local_source","name":"Single","path":single,"selectedPaths":[single],"revision":f.engine.snapshot().unwrap().revision})).await;
    assert_eq!(saved["snapshot"]["skills"][0]["name"], "plain-skill");
    assert_eq!(
        fs::read_to_string(f.target.join("plain-skill/SKILL.md")).unwrap(),
        "Updated instructions"
    );
}

#[tokio::test]
async fn previously_merged_source_can_sync_independently_without_changing_primary() {
    let f = Fixture::new().await;
    let original = f.save().await;
    let other = f.base.join("other");
    write_skill(&other.join("alpha"), "alpha", "original");
    fs::create_dir_all(other.join("shared")).unwrap();
    fs::write(other.join("shared/helper.txt"), "shared-original").unwrap();
    let saved = run(&f.engine, json!({"action":"save_local_source","name":"Other","path":other,"selectedPaths":[other.join("alpha")],"revision":f.engine.snapshot().unwrap().revision})).await;
    f.apply(&saved["sourceId"]).await;
    write_skill(&other.join("alpha"), "alpha", "independent update");
    let updated = run(&f.engine, json!({"action":"save_local_source","sourceId":saved["sourceId"],"name":"Other","path":other,"selectedPaths":[other.join("alpha")],"revision":f.engine.snapshot().unwrap().revision,"keepConflicts":true})).await;
    assert_eq!(updated["snapshot"]["skills"].as_array().unwrap().len(), 2);
    let original_id = &original["snapshot"]["skills"][0]["id"];
    let original_skill = updated["snapshot"]["skills"]
        .as_array()
        .unwrap()
        .iter()
        .find(|skill| &skill["id"] == original_id)
        .unwrap();
    assert_eq!(
        original_skill["bundleDigest"],
        original["snapshot"]["skills"][0]["bundleDigest"]
    );
    assert!(
        fs::read_to_string(f.target.join("alpha/SKILL.md"))
            .unwrap()
            .ends_with("independent update")
    );
}

#[tokio::test]
async fn undo_local_sync_removes_added_source_members_and_keeps_source_distributable() {
    let f = Fixture::new().await;
    let first = f.save().await;
    f.apply(&first["sourceId"]).await;
    write_skill(&f.alpha, "alpha", "update");
    let beta = f.source.join("beta");
    write_skill(&beta, "beta", "added");
    let changed = run(&f.engine, json!({"action":"save_local_source","sourceId":first["sourceId"],"name":"Local","path":f.source,"selectedPaths":[f.alpha,beta],"revision":f.engine.snapshot().unwrap().revision})).await;
    let restored = run(&f.engine, json!({"action":"undo_content_update","sourceId":first["sourceId"],"expectedRevision":changed["snapshot"]["revision"]})).await;
    assert_eq!(
        restored["sources"][0]["localMemberIds"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert!(
        fs::read_to_string(f.target.join("alpha/SKILL.md"))
            .unwrap()
            .ends_with("original")
    );
    assert!(!f.target.join("beta").exists());
    f.apply(&first["sourceId"]).await;
}

#[tokio::test]
async fn sync_cannot_introduce_a_declared_name_conflict_in_an_existing_target() {
    let f = Fixture::new().await;
    let first = f.save().await;
    f.apply(&first["sourceId"]).await;
    let other = f.base.join("other");
    write_skill(&other.join("beta"), "beta", "other source");
    let second = run(&f.engine, json!({"action":"save_local_source","name":"Other","path":other,"selectedPaths":[other.join("beta")],"revision":f.engine.snapshot().unwrap().revision})).await;
    f.apply(&second["sourceId"]).await;
    write_skill(&f.alpha, "beta", "conflicting rename");
    let request = previewed(&f.engine, json!({"action":"save_local_source","sourceId":first["sourceId"],"name":"Local","path":f.source,"selectedPaths":[f.alpha],"revision":f.engine.snapshot().unwrap().revision,"keepConflicts":true})).await;
    let result = f.engine.execute(request).await;
    assert!(
        result.is_err(),
        "updating active content must also enforce target name uniqueness"
    );
    assert!(
        fs::read_to_string(f.target.join("alpha/SKILL.md"))
            .unwrap()
            .ends_with("original")
    );
    assert!(
        fs::read_to_string(f.target.join("beta/SKILL.md"))
            .unwrap()
            .ends_with("other source")
    );
}

#[tokio::test]
async fn shared_resources_changed_after_preview_require_a_new_confirmation() {
    let f = Fixture::new().await;
    let first = f.save().await;
    f.apply(&first["sourceId"]).await;
    let request = previewed(&f.engine, json!({"action":"save_local_source","sourceId":first["sourceId"],"name":"Local","path":f.source,"selectedPaths":[f.alpha],"revision":f.engine.snapshot().unwrap().revision})).await;
    let revision = f.engine.snapshot().unwrap().revision;
    let entity = fs::canonicalize(f.target.join("alpha")).unwrap();
    fs::write(f.source.join("shared/helper.txt"), "unreviewed change").unwrap();
    let error = f.engine.execute(request).await.unwrap_err();
    assert!(error.to_string().contains("预览后发生变化"));
    assert_eq!(f.engine.snapshot().unwrap().revision, revision);
    assert_eq!(fs::canonicalize(f.target.join("alpha")).unwrap(), entity);
    assert_eq!(
        fs::read_to_string(entity.parent().unwrap().join("shared/helper.txt")).unwrap(),
        "shared-original"
    );
}
