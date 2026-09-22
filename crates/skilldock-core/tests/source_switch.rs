use serde_json::{Value, json};
use skilldock_core::Engine;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};
async fn run(e: &Engine, r: Value) -> Value {
    e.execute(r).await.unwrap()
}
fn skill(path: &Path, body: &str) {
    fs::create_dir_all(path).unwrap();
    fs::write(
        path.join("SKILL.md"),
        format!("---\nname: alpha\ndescription: test\n---\n{body}"),
    )
    .unwrap();
}
fn link(from: &Path, to: &Path) {
    #[cfg(unix)]
    std::os::unix::fs::symlink(from, to).unwrap();
    #[cfg(windows)]
    std::os::windows::fs::symlink_dir(from, to).unwrap();
}
struct F {
    _temp: tempfile::TempDir,
    e: Engine,
    base: PathBuf,
    state: Value,
    old: Value,
    new: Value,
    source: Value,
    bid: Value,
    tid: Value,
    other: Value,
    entry: PathBuf,
    old_entity: PathBuf,
    new_entity: PathBuf,
}
impl F {
    async fn new(git: bool, same: bool) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let base = fs::canonicalize(temp.path()).unwrap();
        let e = Engine::new(Some(base.join("config"))).unwrap();
        let mut legacy = e.configure(base.join("library").to_str().unwrap()).unwrap();
        // Existing libraries keep per-target source choices until explicit migration.
        legacy.schema_version = 2;
        skilldock_core::files::atomic_json(
            &PathBuf::from(&legacy.storage_root).join("state.json"),
            &legacy,
        )
        .unwrap();
        let target = base.join("tool");
        let entry = target.join("legacy-entry");
        skill(&entry, "original");
        let mut state = run(
            &e,
            json!({"action":"import_folder","path":target,"selectedPaths":[entry],"adopt":true}),
        )
        .await;
        let old = state["skills"][0]["id"].clone();
        let bid = state["bindings"][0]["id"].clone();
        let tid = state["bindings"][0]["targetId"].clone();
        let old_entity = fs::canonicalize(&entry).unwrap();
        let other_path = base.join("other-tool");
        fs::create_dir(&other_path).unwrap();
        state = run(
            &e,
            json!({"action":"add_target","path":other_path,"name":"Other"}),
        )
        .await;
        let other = state["targets"]
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["path"] == other_path.display().to_string())
            .unwrap()["id"]
            .clone();
        state=run(&e,json!({"action":"distribute","skillIds":[old],"targetIds":[other],"expectedRevision":state["revision"]})).await;
        let source_path = base.join("new-source");
        let alpha = source_path.join("alpha");
        skill(&alpha, if same { "original" } else { "new content" });
        let source;
        if git {
            for args in [
                vec!["init", "-q"],
                vec!["add", "."],
                vec![
                    "-c",
                    "user.name=Test",
                    "-c",
                    "user.email=test@example.invalid",
                    "-c",
                    "commit.gpgsign=false",
                    "commit",
                    "-qm",
                    "fixture",
                ],
            ] {
                let output = Command::new("git")
                    .args(args)
                    .current_dir(&source_path)
                    .output()
                    .unwrap();
                assert!(
                    output.status.success(),
                    "{}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
            state = run(
                &e,
                json!({"action":"import_git","url":source_path,"reference":"HEAD","subdir":""}),
            )
            .await;
            source = state["sources"]
                .as_array()
                .unwrap()
                .iter()
                .find(|s| s["kind"] == "git")
                .unwrap()["id"]
                .clone();
        } else {
            let result=run(&e,json!({"action":"save_local_source","name":"Local B","path":source_path,"selectedPaths":[alpha],"revision":state["revision"]})).await;
            source = result["sourceId"].clone();
            state = result["snapshot"].clone();
        }
        let new_skill = state["skills"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["sourceId"] == source)
            .unwrap();
        let new = new_skill["id"].clone();
        let new_entity = if git {
            base.join("library/.skilldock/objects")
                .join(new_skill["bundleDigest"].as_str().unwrap())
                .join("tree")
                .join(new_skill["relativePath"].as_str().unwrap())
        } else {
            alpha
        };
        Self {
            _temp: temp,
            e,
            base,
            state,
            old,
            new,
            source,
            bid,
            tid,
            other,
            entry,
            old_entity,
            new_entity,
        }
    }
    fn request(&self, action: &str, confirmed: bool) -> Value {
        json!({"action":action,"sourceId":self.source,"skillIds":[self.new],"targetIds":[self.tid],"expectedRevision":self.state["revision"],"replaceBindingIds":if confirmed {vec![self.bid.clone()]} else {vec![]}})
    }
    async fn plan(&self, claim: &str, confirmed: bool) -> Value {
        let mut r = self.request("plan", confirmed);
        r["claim"] = json!(claim);
        run(&self.e, r).await
    }
    fn binding(&self) -> &Value {
        self.state["bindings"]
            .as_array()
            .unwrap()
            .iter()
            .find(|b| b["id"] == self.bid)
            .unwrap()
    }
}
#[tokio::test]
async fn collected_install_switches_to_local_source_without_deleting_old_entity_or_other_targets() {
    let mut f = F::new(false, false).await;
    let claim = format!("source:{}", f.source.as_str().unwrap());
    let preview = f.plan(&claim, false).await;
    assert_eq!(preview["items"][0]["replacement"]["contentEqual"], false);
    assert_eq!(preview["items"][0]["replacement"]["bindingId"], f.bid);
    assert_eq!(
        preview["items"][0]["path"],
        f.entry.display().to_string(),
        "reuse original entry name"
    );
    assert!(
        f.e.execute(f.request("apply_local_source", false))
            .await
            .is_err()
    );
    assert_eq!(fs::canonicalize(&f.entry).unwrap(), f.old_entity);
    let confirmed = f.plan(&claim, true).await;
    assert_eq!(confirmed["items"][0]["action"], "replace");
    f.state = run(&f.e, f.request("apply_local_source", true)).await;
    assert_eq!(f.binding()["skillId"], f.new);
    assert_eq!(f.binding()["follow"], false);
    assert!(
        f.binding()["claims"]
            .as_array()
            .unwrap()
            .contains(&json!("manual"))
    );
    assert_eq!(fs::canonicalize(&f.entry).unwrap(), f.new_entity);
    assert!(f.old_entity.join("SKILL.md").is_file());
    assert!(fs::read_dir(f.entry.parent().unwrap()).unwrap().any(|e| {
        e.unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".skilldock-backup-")
    }));
    assert_eq!(
        f.state["bindings"]
            .as_array()
            .unwrap()
            .iter()
            .find(|b| b["targetId"] == f.other)
            .unwrap()["skillId"],
        f.old
    );
    f.state=run(&f.e,json!({"action":"revoke_local_source","sourceId":f.source,"targetIds":[f.tid],"expectedRevision":f.state["revision"]})).await;
    assert!(
        f.entry.is_symlink(),
        "manual claim survives source cancellation"
    );
    run(&f.e, json!({"action":"revoke","bindingIds":[f.bid]})).await;
    assert!(!f.entry.is_symlink());
    assert!(f.old_entity.is_dir());
    assert!(f.new_entity.is_dir());
}
#[tokio::test]
async fn git_source_and_same_preset_replacement_keep_membership_consistent() {
    let mut f = F::new(true, true).await;
    f.state = run(
        &f.e,
        json!({"action":"save_preset","name":"Workflow","skillIds":[f.old]}),
    )
    .await;
    let pid = f.state["presets"][0]["id"].clone();
    f.state=run(&f.e,json!({"action":"apply_preset","presetId":pid,"targetIds":[f.tid],"expectedRevision":f.state["revision"]})).await;
    f.state = run(
        &f.e,
        json!({"action":"save_preset","id":pid,"name":"Workflow","skillIds":[f.new]}),
    )
    .await;
    let claim = format!("preset:{}", pid.as_str().unwrap());
    let preview = f.plan(&claim, false).await;
    assert_eq!(preview["items"][0]["replacement"]["contentEqual"], true);
    assert_eq!(
        preview["items"][0]["replacement"]["blockingClaims"],
        json!([])
    );
    let mut request = f.request("apply_preset", true);
    request["presetId"] = pid.clone();
    f.state = run(&f.e, request).await;
    assert_eq!(fs::canonicalize(&f.entry).unwrap(), f.new_entity);
    assert_eq!(f.binding()["skillId"], f.new);
    assert_eq!(f.state["presets"][0]["skillIds"], json!([f.new]));
    assert_eq!(
        f.state["presetApplications"][0]["appliedRevision"],
        f.state["presets"][0]["revision"]
    );
    assert!(f.old_entity.is_dir());
    // Subsequent apply reuses the new identity, with no replacement authorization needed.
    f.state=run(&f.e,json!({"action":"apply_preset","presetId":pid,"targetIds":[f.tid],"expectedRevision":f.state["revision"]})).await;
    assert_eq!(f.binding()["skillId"], f.new);
}
#[tokio::test]
async fn other_preset_claims_block_switch_even_when_explicitly_requested() {
    let mut f = F::new(false, false).await;
    f.state = run(
        &f.e,
        json!({"action":"save_preset","name":"Protected preset","skillIds":[f.old]}),
    )
    .await;
    let pid = f.state["presets"][0]["id"].clone();
    let claim = format!("preset:{}", pid.as_str().unwrap());
    f.state=run(&f.e,json!({"action":"apply_preset","presetId":pid,"targetIds":[f.tid],"expectedRevision":f.state["revision"]})).await;
    let preview = f.plan("manual", true).await;
    assert_eq!(
        preview["items"][0]["replacement"]["blockingClaims"],
        json!([claim])
    );
    assert!(f.e.execute(f.request("distribute", true)).await.is_err());
    assert_eq!(fs::canonicalize(&f.entry).unwrap(), f.old_entity);
    run(
        &f.e,
        json!({"action":"revoke_preset","presetId":pid,"targetIds":[f.tid]}),
    )
    .await;
    f.state = serde_json::to_value(f.e.snapshot().unwrap()).unwrap();
    f.state = run(&f.e, f.request("distribute", true)).await;
    assert_eq!(f.binding()["skillId"], f.new);
}
#[tokio::test]
async fn stale_or_tampered_links_cannot_be_switched_and_batches_are_atomic() {
    let mut f = F::new(false, false).await;
    let stale = f.request("distribute", true);
    f.state = run(
        &f.e,
        json!({"action":"save_preset","name":"Unapplied","skillIds":[f.old]}),
    )
    .await;
    assert!(f.e.execute(stale).await.is_err());
    let mut no_consent = f.request("distribute", false);
    no_consent["takeover"] = json!(true);
    no_consent["adoptExisting"] = json!(true);
    assert!(
        f.e.execute(no_consent).await.is_err(),
        "other permissions do not authorize a source switch"
    );
    let mut invalid = f.request("distribute", true);
    invalid["replaceBindingIds"] = json!(["missing"]);
    assert!(f.e.execute(invalid).await.is_err());
    let other_binding = f.state["bindings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["targetId"] == f.other)
        .unwrap();
    let other_entry = PathBuf::from(other_binding["path"].as_str().unwrap());
    let other_bid = other_binding["id"].clone();
    fs::remove_file(&other_entry).unwrap();
    let external = f.base.join("external");
    skill(&external, "external");
    link(&external, &other_entry);
    let mut batch = f.request("distribute", true);
    batch["targetIds"] = json!([f.tid, f.other]);
    batch["replaceBindingIds"] = json!([f.bid, other_bid]);
    assert!(f.e.execute(batch).await.is_err());
    assert_eq!(
        fs::canonicalize(&f.entry).unwrap(),
        f.old_entity,
        "valid target must not be partially switched"
    );
    assert_eq!(fs::canonicalize(&other_entry).unwrap(), external);
    fs::remove_file(&f.entry).unwrap();
    fs::create_dir(&f.entry).unwrap();
    fs::write(f.entry.join("keep"), "keep").unwrap();
    assert!(f.e.execute(f.request("distribute", true)).await.is_err());
    assert_eq!(fs::read_to_string(f.entry.join("keep")).unwrap(), "keep");
}

#[tokio::test]
async fn local_source_claim_blocks_replacement_by_another_source() {
    let mut f = F::new(false, false).await;
    f.state = run(&f.e, f.request("apply_local_source", true)).await;
    let next = f.base.join("third-source/alpha");
    skill(&next, "third");
    let result=run(&f.e,json!({"action":"save_local_source","name":"Third","path":next.parent(),"selectedPaths":[next],"revision":f.state["revision"]})).await;
    f.state = result["snapshot"].clone();
    let third = f.state["skills"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["sourceId"] == result["sourceId"])
        .unwrap()["id"]
        .clone();
    let request =
        json!({"action":"plan","skillIds":[third],"targetIds":[f.tid],"replaceBindingIds":[f.bid]});
    let preview = run(&f.e, request).await;
    assert_eq!(
        preview["items"][0]["replacement"]["blockingClaims"],
        json!([format!("source:{}", f.source.as_str().unwrap())])
    );
    let blocked = json!({"action":"apply_local_source","sourceId":result["sourceId"],"targetIds":[f.tid],"replaceBindingIds":[f.bid],"expectedRevision":f.state["revision"]});
    assert!(f.e.execute(blocked).await.is_err());
    assert_eq!(fs::canonicalize(&f.entry).unwrap(), f.new_entity);
}

#[tokio::test]
async fn switching_a_borrowed_install_restores_the_original_link_on_cancellation() {
    let mut f = F::new(false, false).await;
    let target = f.base.join("borrowed-tool");
    fs::create_dir(&target).unwrap();
    let entry = target.join("alpha");
    link(&f.new_entity, &entry);
    f.state = run(
        &f.e,
        json!({"action":"add_target","name":"Borrowed","path":target}),
    )
    .await;
    let tid = f.state["targets"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["path"] == target.display().to_string())
        .unwrap()["id"]
        .clone();
    f.state=run(&f.e,json!({"action":"distribute","skillIds":[f.new],"targetIds":[tid],"expectedRevision":f.state["revision"]})).await;
    let bid = f.state["bindings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["targetId"] == tid)
        .unwrap()["id"]
        .clone();
    let preview = run(
        &f.e,
        json!({"action":"plan","skillIds":[f.old],"targetIds":[tid],"replaceBindingIds":[bid]}),
    )
    .await;
    assert_eq!(preview["items"][0]["replacement"]["restoresOriginal"], true);
    f.state=run(&f.e,json!({"action":"distribute","skillIds":[f.old],"targetIds":[tid],"expectedRevision":preview["revision"],"replaceBindingIds":[bid]})).await;
    assert_eq!(fs::canonicalize(&entry).unwrap(), f.old_entity);
    run(&f.e, json!({"action":"revoke","bindingIds":[bid]})).await;
    assert_eq!(fs::canonicalize(&entry).unwrap(), f.new_entity);
    assert!(f.new_entity.join("SKILL.md").is_file());
}
