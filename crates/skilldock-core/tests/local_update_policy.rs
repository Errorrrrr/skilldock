use serde_json::json;
use skilldock_core::Engine;
use std::{fs, process::Command};

#[tokio::test]
async fn local_git_package_stays_manual_and_legacy_policy_is_disabled() {
    let temp = tempfile::tempdir().unwrap();
    let base = fs::canonicalize(temp.path()).unwrap();
    let repo = base.join("repo");
    fs::create_dir_all(repo.join("alpha")).unwrap();
    fs::write(
        repo.join("alpha/SKILL.md"),
        "---\nname: alpha\ndescription: test\n---\ncontent",
    )
    .unwrap();
    assert!(
        Command::new("git")
            .args(["init", "--quiet"])
            .arg(&repo)
            .status()
            .unwrap()
            .success()
    );
    assert!(
        Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args([
                "remote",
                "add",
                "origin",
                "git@git.example.com:group/repo.git"
            ])
            .status()
            .unwrap()
            .success()
    );
    let engine = Engine::new(Some(base.join("config"))).unwrap();
    let library = base.join("library");
    engine.configure(library.to_str().unwrap()).unwrap();
    let revision = engine.snapshot().unwrap().revision;
    let result = engine
        .execute(json!({"action":"import_package", "path":repo, "revision":revision}))
        .await
        .unwrap();
    let before = engine.snapshot().unwrap();
    let source_id = before.sources[0].id.clone();
    assert_eq!(before.sources[0].kind, "local");
    assert!(!before.sources[0].supports_remote_updates());
    assert!(engine.execute(json!({"action":"set_policy", "sourceId":source_id, "mode":"auto", "intervalHours":1})).await.is_err());
    // Simulate the old .git inference without contacting the remote.
    let mut legacy = before.clone();
    legacy.sources[0].kind = "git".into();
    legacy.sources[0].url = "git@git.example.com:group/repo.git".into();
    legacy.sources[0].policy.mode = "auto".into();
    legacy.sources[0].next_check = "2000-01-01T00:00:00Z".into();
    fs::write(
        library.join("state.json"),
        serde_json::to_vec(&legacy).unwrap(),
    )
    .unwrap();
    let migrated = engine.snapshot().unwrap();
    assert_eq!(migrated.sources[0].kind, "local");
    assert_eq!(migrated.sources[0].policy.mode, "off");
    assert!(migrated.sources[0].next_check.is_empty());
    assert_eq!(
        serde_json::to_value(&migrated.skills).unwrap(),
        result["snapshot"]["skills"]
    );
    engine.run_due_updates().await.unwrap();
    fs::create_dir_all(repo.join("beta")).unwrap();
    fs::write(
        repo.join("beta/SKILL.md"),
        "---\nname: beta\ndescription: test\n---\ncontent",
    )
    .unwrap();
    assert_eq!(engine.snapshot().unwrap().skills.len(), 1);
    let revision = engine.snapshot().unwrap().revision;
    engine
        .execute(json!({"action":"import_package", "path":repo, "revision":revision}))
        .await
        .unwrap();
    assert_eq!(engine.snapshot().unwrap().skills.len(), 2);
}
