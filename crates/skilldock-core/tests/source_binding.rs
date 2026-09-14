use serde_json::json;
use skilldock_core::Engine;
use std::fs;

#[tokio::test]
async fn collected_directory_cannot_be_bound_to_a_shared_upstream() {
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
        assert!(engine
            .execute(
                json!({"action":"preview_bind_source","sourceId":sid,"kind":"local","path":path})
            )
            .await
            .is_err());
    }
    let incomplete = base.join("incomplete");
    fs::create_dir_all(&incomplete).unwrap();
    assert!(engine
        .execute(
            json!({"action":"preview_bind_source","sourceId":sid,"kind":"local","path":incomplete})
        )
        .await
        .is_err());
    // Even a repository with matching names cannot prove common provenance.
    for action in ["preview_bind_source", "bind_source"] {
        for kind in ["local", "git"] {
            let error = engine
                .execute(json!({
                    "action": action, "sourceId": sid, "kind": kind,
                    "path": upstream, "url": "https://example.invalid/skills.git"
                }))
                .await
                .unwrap_err();
            assert!(error.to_string().contains("归集记录不是统一更新来源"));
        }
    }
    let after = engine.snapshot().unwrap();
    let after = serde_json::to_value(after).unwrap();
    assert_eq!(before["skills"], after["skills"]);
    assert_eq!(before["bindings"], after["bindings"]);
    assert_eq!(before["sources"], after["sources"]);
    assert_eq!(before["revision"], after["revision"]);
    assert_eq!(
        fs::read_to_string(source.join("contact/SKILL.md")).unwrap(),
        content
    );
}
