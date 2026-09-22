use serde_json::{Value, json};
use skilldock_core::Engine;
use std::fs;

async fn run(engine: &Engine, request: Value) -> Value {
    engine.execute(request).await.unwrap()
}

#[tokio::test]
async fn reference_source_preserves_original_links_and_other_claims() {
    let temp = tempfile::tempdir().unwrap();
    let base = fs::canonicalize(temp.path()).unwrap();
    let library = base.join("library");
    let source = base.join("source");
    let alpha = source.join("alpha");
    let target = base.join("target");
    let existing = base.join("existing");
    for directory in [&alpha, &target, &existing] {
        fs::create_dir_all(directory).unwrap();
    }
    fs::write(
        alpha.join("SKILL.md"),
        "---\nname: alpha\ndescription: test\n---\ncontent",
    )
    .unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(&alpha, existing.join("alpha")).unwrap();
    #[cfg(windows)]
    std::os::windows::fs::symlink_dir(&alpha, existing.join("alpha")).unwrap();
    let engine = Engine::new(Some(base.join("config"))).unwrap();
    engine.configure(library.to_str().unwrap()).unwrap();
    let t = run(
        &engine,
        json!({"action":"add_target","path":target,"name":"new"}),
    )
    .await;
    let tid = t["targets"][0]["id"].clone();
    let t = run(
        &engine,
        json!({"action":"add_target","path":existing,"name":"existing"}),
    )
    .await;
    let eid = t["targets"][1]["id"].clone();
    let revision = t["revision"].clone();
    let request = json!({"action":"save_local_source","path":source,"name":"Local", "selectedPaths":[alpha],"revision":revision});
    let result = run(&engine, request.clone()).await;
    let sid = result["sourceId"].clone();
    let skill_id = result["snapshot"]["skills"][0]["id"].clone();
    assert!(
        result["snapshot"]["bindings"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        result["snapshot"]["skills"][0]["externalPath"],
        alpha.display().to_string()
    );
    assert_eq!(result["snapshot"]["skills"][0]["bundleDigest"], "");
    assert!(
        engine.execute(request).await.is_err(),
        "stale save must fail"
    );
    let mut state = run(&engine, json!({"action":"apply_local_source", "sourceId":sid,"targetIds":[tid,eid],"expectedRevision":result["snapshot"]["revision"]})).await;
    assert_eq!(fs::canonicalize(target.join("alpha")).unwrap(), alpha);
    state = run(&engine, json!({"action":"distribute","skillIds":[skill_id],"targetIds":[tid],"expectedRevision":state["revision"]})).await;
    assert!(
        engine
            .execute(json!({"action":"remove_skill","skillId":skill_id}))
            .await
            .is_err()
    );
    state = run(&engine, json!({"action":"revoke_local_source","sourceId":sid,"targetIds":[tid,eid],"expectedRevision":state["revision"]})).await;
    assert!(
        target.join("alpha").is_symlink(),
        "manual claim preserves new link"
    );
    assert!(
        existing.join("alpha").is_symlink(),
        "borrowed original link is retained"
    );
    let before = state["skills"].clone();
    state = run(&engine, json!({"action":"remove_local_source","sourceId":sid,"targetIds":[],"expectedRevision":state["revision"]})).await;
    assert_eq!(state["skills"], before);
    assert_eq!(state["sources"][0]["updatesRemoved"], true);
    assert!(alpha.join("SKILL.md").is_file());
    assert!(
        fs::read_dir(library.join(".skilldock/objects"))
            .unwrap()
            .next()
            .is_none()
    );
}
