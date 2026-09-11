use serde_json::json;
use skilldock_core::Engine;
use std::fs;

#[tokio::test]
async fn detached_source_can_bind_verified_original_without_changing_installations() {
    let temp = tempfile::tempdir().unwrap();
    let base = fs::canonicalize(temp.path()).unwrap();
    let engine = Engine::new(Some(base.join("config"))).unwrap();
    let root = base.join("library");
    let source = base.join("installed");
    let upstream = base.join("upstream");
    fs::create_dir_all(source.join("contact")).unwrap();
    fs::create_dir_all(upstream.join("contact")).unwrap();
    let content = "---\nname: contact\ndescription: test\n---\n# original";
    fs::write(source.join("contact/SKILL.md"), content).unwrap();
    fs::write(
        upstream.join("contact/SKILL.md"),
        format!("{content}\nupdated"),
    )
    .unwrap();
    engine.configure(root.to_str().unwrap()).unwrap();
    let before = engine.execute(json!({"action":"import_folder","path":source,"selectedPaths":[source.join("contact")],"adopt":true})).await.unwrap();
    let sid = before["sources"][0]["id"].as_str().unwrap();
    assert_eq!(before["sources"][0]["status"], "detached");
    for path in [&source, &root, &root.join("objects")] {
        assert!(engine.execute(json!({"action":"preview_bind_source","sourceId":sid,"kind":"local","path":path})).await.is_err());
    }
    let incomplete = base.join("incomplete");
    fs::create_dir_all(&incomplete).unwrap();
    assert!(engine.execute(json!({"action":"preview_bind_source","sourceId":sid,"kind":"local","path":incomplete})).await.is_err());
    let mut request =
        json!({"action":"preview_bind_source","sourceId":sid,"kind":"local","path":upstream});
    let preview = engine.execute(request.clone()).await.unwrap();
    assert_eq!(preview["memberNames"], json!(["contact"]));
    request["action"] = json!("bind_source");
    request["expectedRevision"] = preview["revision"].clone();
    request["digest"] = preview["digest"].clone();
    fs::write(
        upstream.join("contact/SKILL.md"),
        format!("{content}\nchanged after preview"),
    )
    .unwrap();
    assert!(engine.execute(request.clone()).await.is_err());
    request["action"] = json!("preview_bind_source");
    let preview = engine.execute(request.clone()).await.unwrap();
    request["action"] = json!("bind_source");
    request["digest"] = preview["digest"].clone();
    request["expectedRevision"] = json!(0);
    assert!(engine.execute(request.clone()).await.is_err());
    request["expectedRevision"] = preview["revision"].clone();
    let after = engine.execute(request).await.unwrap();
    assert_eq!(before["skills"], after["skills"]);
    assert_eq!(before["bindings"], after["bindings"]);
    assert_eq!(before["presets"], after["presets"]);
    assert_eq!(after["sources"][0]["path"], upstream.display().to_string());
    assert_eq!(after["sources"][0]["status"], "available");
    assert_eq!(after["sources"][0]["policy"]["mode"], "off");
    let checked = engine
        .execute(json!({"action":"check_source","sourceId":sid,"apply":false}))
        .await
        .unwrap();
    assert_eq!(checked["sources"][0]["status"], "available");
    let updated = engine
        .execute(json!({"action":"check_source","sourceId":sid,"apply":true}))
        .await
        .unwrap();
    assert_ne!(
        updated["skills"][0]["bundleDigest"],
        before["skills"][0]["bundleDigest"]
    );
    assert_eq!(
        fs::read_to_string(source.join("contact/SKILL.md")).unwrap(),
        content
    );
}
