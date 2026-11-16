use serde_json::{Value, json};
use skilldock_core::{Engine, files};
use std::{fs, path::PathBuf, process::Command};

struct Fixture {
    _temp: tempfile::TempDir,
    engine: Engine,
    source: PathBuf,
    target: PathBuf,
    source_id: String,
}
impl Fixture {
    fn git(&self, args: &[&str]) {
        let output = Command::new("git")
            .args(args)
            .current_dir(&self.source)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    async fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let base = fs::canonicalize(temp.path()).unwrap();
        let engine = Engine::new(Some(base.join("config"))).unwrap();
        engine
            .configure(base.join("library").to_str().unwrap())
            .unwrap();
        let source = base.join("source");
        let target = base.join("target");
        fs::create_dir_all(source.join("alpha")).unwrap();
        fs::create_dir_all(&target).unwrap();
        fs::write(source.join("alpha/SKILL.md"), "---\nname: alpha\n---\nold").unwrap();
        let mut f = Self {
            _temp: temp,
            engine,
            source,
            target,
            source_id: String::new(),
        };
        f.git(&["init", "-q"]);
        f.git(&["add", "."]);
        f.git(&[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.com",
            "commit",
            "-qm",
            "initial",
        ]);
        let state = f
            .engine
            .execute(json!({"action":"import_git","url":format!("https://skilldock-test.invalid{}", f.source.display()),"reference":"HEAD","subdir":""}))
            .await
            .unwrap();
        f.source_id = state["sources"][0]["id"].as_str().unwrap().into();
        let skill_id = state["skills"][0]["id"].clone();
        let state = f
            .engine
            .execute(json!({"action":"add_target","name":"Tool","path":f.target}))
            .await
            .unwrap();
        f.engine.execute(json!({"action":"distribute","skillIds":[skill_id],"targetIds":[state["targets"][0]["id"]],"expectedRevision":state["revision"]})).await.unwrap();
        f.engine.execute(json!({"action":"set_policy","sourceId":f.source_id,"mode":"auto","dailyTime":"09:00"})).await.unwrap();
        f
    }
    fn change_source(&self) {
        fs::write(
            self.source.join("alpha/SKILL.md"),
            "---\nname: alpha\n---\nnew",
        )
        .unwrap();
        self.git(&["add", "."]);
        self.git(&[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.com",
            "commit",
            "-qm",
            "update",
        ]);
    }
    fn due(&self) -> String {
        let mut state = self.engine.snapshot().unwrap();
        let due = "2000-01-01T01:00:00Z".to_string();
        state.sources[0].next_check = due.clone();
        files::atomic_json(
            &PathBuf::from(&state.storage_root).join("state.json"),
            &state,
        )
        .unwrap();
        due
    }
    fn replace_link(&self) {
        files::remove_link(&self.target.join("alpha")).unwrap();
        fs::create_dir(self.target.join("alpha")).unwrap();
        fs::write(self.target.join("alpha/SKILL.md"), "local edits").unwrap();
        fs::write(self.target.join("alpha/local.txt"), "keep me").unwrap();
    }
    async fn preview(&self) -> Value {
        self.engine
            .execute(json!({"action":"preview_single_content"}))
            .await
            .unwrap()
    }
    async fn repair(&self, preview: &Value) -> Result<Value, skilldock_core::error::Error> {
        self.engine.execute(json!({"action":"repair_distribution_links","expectedRevision":preview["revision"],"directoryRepairs":preview["directoryRepairs"]})).await
    }
}

async fn check_only_does_not_consume_overdue_automatic_application() {
    let f = Fixture::new().await;
    f.change_source();
    let due = f.due();
    let checked = f.engine.check_source(&f.source_id, false).await.unwrap();
    assert_eq!(checked.sources[0].status, "available");
    assert_eq!(checked.sources[0].next_check, due);
    assert!(
        fs::read_to_string(f.target.join("alpha/SKILL.md"))
            .unwrap()
            .ends_with("old")
    );
    f.engine.run_due_updates().await.unwrap();
    let updated = f.engine.snapshot().unwrap();
    assert_eq!(updated.sources[0].status, "current");
    assert_ne!(updated.sources[0].next_check, due);
    assert!(
        fs::read_to_string(f.target.join("alpha/SKILL.md"))
            .unwrap()
            .ends_with("new")
    );
}

async fn failed_manual_check_still_allows_overdue_automatic_update() {
    let f = Fixture::new().await;
    f.change_source();
    let due = f.due();
    let unavailable = f.source.with_extension("unavailable");
    fs::rename(&f.source, &unavailable).unwrap();
    assert!(f.engine.check_source(&f.source_id, false).await.is_err());
    let failed = f.engine.snapshot().unwrap();
    assert_eq!(failed.sources[0].status, "error");
    assert_eq!(failed.sources[0].next_check, due);
    fs::rename(unavailable, &f.source).unwrap();
    f.engine.run_due_updates().await.unwrap();
    let updated = f.engine.snapshot().unwrap();
    assert_eq!(updated.sources[0].status, "current");
    assert_ne!(updated.sources[0].next_check, due);
    assert!(
        fs::read_to_string(f.target.join("alpha/SKILL.md"))
            .unwrap()
            .ends_with("new")
    );
}

async fn notification_policy_advances_without_applying() {
    let f = Fixture::new().await;
    f.engine.execute(json!({"action":"set_policy","sourceId":f.source_id,"mode":"notify","dailyTime":"09:00"})).await.unwrap();
    f.change_source();
    let due = f.due();
    f.engine.run_due_updates().await.unwrap();
    let checked = f.engine.snapshot().unwrap();
    assert_eq!(checked.sources[0].status, "available");
    assert_ne!(checked.sources[0].next_check, due);
    assert!(
        fs::read_to_string(f.target.join("alpha/SKILL.md"))
            .unwrap()
            .ends_with("old")
    );
}

async fn scheduled_apply_records_conflict_then_confirmed_repair_allows_update() {
    let f = Fixture::new().await;
    let before = f.engine.snapshot().unwrap();
    f.change_source();
    f.replace_link();
    f.due();
    f.engine.run_due_updates().await.unwrap();
    let failed = f.engine.snapshot().unwrap();
    assert_eq!(failed.sources[0].status, "error");
    assert!(failed.sources[0].error.contains("检查并修复分发链接"));
    assert_eq!(
        failed.skills[0].bundle_digest,
        before.skills[0].bundle_digest
    );
    assert_eq!(
        fs::read_to_string(f.target.join("alpha/SKILL.md")).unwrap(),
        "local edits"
    );
    let preview = f.preview().await;
    assert_eq!(preview["alreadyEnabled"], true);
    assert_eq!(preview["directoryRepairs"].as_array().unwrap().len(), 1);
    let error = f
        .engine
        .execute(
            json!({"action":"repair_distribution_links","expectedRevision":preview["revision"]}),
        )
        .await
        .unwrap_err();
    assert!(error.to_string().contains("重新预览"));
    f.repair(&preview).await.unwrap();
    assert!(f.target.join("alpha").is_symlink());
    let backup = fs::read_dir(&f.target)
        .unwrap()
        .map(|e| e.unwrap().path())
        .find(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with(".skilldock-backup-")
        })
        .unwrap();
    assert_eq!(
        fs::read_to_string(backup.join("local.txt")).unwrap(),
        "keep me"
    );
    assert_eq!(
        fs::read_to_string(backup.join("SKILL.md")).unwrap(),
        "local edits"
    );
    let state = f.engine.check_source(&f.source_id, true).await.unwrap();
    assert_eq!(state.sources[0].status, "current");
    assert!(
        fs::read_to_string(f.target.join("alpha/SKILL.md"))
            .unwrap()
            .ends_with("new")
    );
    // The existing recovery path must also work for repairs made after migration.
    let backups = f
        .engine
        .execute(json!({"action":"list_backups"}))
        .await
        .unwrap();
    let state = f.engine.snapshot().unwrap();
    f.engine.execute(json!({"action":"restore_backup","backupId":backups[0]["id"],"expectedRevision":state.revision})).await.unwrap();
    assert!(!f.target.join("alpha").is_symlink());
    assert_eq!(
        fs::read_to_string(f.target.join("alpha/local.txt")).unwrap(),
        "keep me"
    );
}

async fn repair_rejects_changed_directory_and_stale_revision() {
    let f = Fixture::new().await;
    f.replace_link();
    let preview = f.preview().await;
    fs::write(f.target.join("alpha/local.txt"), "changed after preview").unwrap();
    assert!(
        f.repair(&preview)
            .await
            .unwrap_err()
            .to_string()
            .contains("重新预览")
    );
    assert!(!f.target.join("alpha").is_symlink());
    let mut preview = f.preview().await;
    preview["revision"] = json!(0);
    assert!(
        f.repair(&preview)
            .await
            .unwrap_err()
            .to_string()
            .contains("重新预览")
    );
    assert_eq!(
        fs::read_to_string(f.target.join("alpha/local.txt")).unwrap(),
        "changed after preview"
    );
}

// Give this test process its own Git URL rewrite. The production network path
// still clones/fetches real commits; all transport stays inside the temp fixture.
#[test]
fn update_distribution_regressions() {
    if std::env::var_os("SKILLDOCK_UPDATE_REGRESSION_CHILD").is_none() {
        let status = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "update_distribution_regressions", "--nocapture"])
            .env("SKILLDOCK_UPDATE_REGRESSION_CHILD", "1")
            .env("GIT_CONFIG_COUNT", "2")
            .env("GIT_CONFIG_KEY_0", "url.file://.insteadOf")
            .env("GIT_CONFIG_VALUE_0", "https://skilldock-test.invalid")
            .env("GIT_CONFIG_KEY_1", "protocol.file.allow")
            .env("GIT_CONFIG_VALUE_1", "always")
            .status()
            .unwrap();
        assert!(status.success());
        return;
    }
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        check_only_does_not_consume_overdue_automatic_application().await;
        notification_policy_advances_without_applying().await;
        failed_manual_check_still_allows_overdue_automatic_update().await;
        scheduled_apply_records_conflict_then_confirmed_repair_allows_update().await;
        repair_rejects_changed_directory_and_stale_revision().await;
    });
}
