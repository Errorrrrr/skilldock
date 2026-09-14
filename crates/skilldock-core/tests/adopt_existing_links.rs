use serde_json::{Value, json};
use skilldock_core::Engine;
use std::{
    fs,
    path::{Path, PathBuf},
};

async fn run(engine: &Engine, request: Value) -> Value {
    engine.execute(request).await.unwrap()
}
fn link(from: &Path, to: &Path) {
    #[cfg(unix)]
    std::os::unix::fs::symlink(from, to).unwrap();
    #[cfg(windows)]
    std::os::windows::fs::symlink_dir(from, to).unwrap();
}
struct Fixture {
    _temp: tempfile::TempDir,
    engine: Engine,
    alpha: PathBuf,
    entry: PathBuf,
    state: Value,
    source_id: Value,
    skill_id: Value,
    target_id: Value,
}
impl Fixture {
    async fn new(relative: bool) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let base = fs::canonicalize(temp.path()).unwrap();
        let source = base.join("source");
        let alpha = source.join("alpha");
        let target = base.join("target");
        fs::create_dir_all(&alpha).unwrap();
        fs::create_dir_all(&target).unwrap();
        fs::write(
            alpha.join("SKILL.md"),
            "---\nname: alpha\ndescription: test\n---\ncontent",
        )
        .unwrap();
        let entry = target.join("alpha");
        link(
            if relative {
                Path::new("../source/alpha")
            } else {
                &alpha
            },
            &entry,
        );
        let engine = Engine::new(Some(base.join("config"))).unwrap();
        engine
            .configure(base.join("library").to_str().unwrap())
            .unwrap();
        let state = run(
            &engine,
            json!({"action":"add_target", "path":target,"name":"Target"}),
        )
        .await;
        let target_id = state["targets"][0]["id"].clone();
        let result = run(&engine, json!({"action":"save_local_source", "path":source,"name":"Local","selectedPaths":[alpha],"revision":state["revision"]})).await;
        let state = result["snapshot"].clone();
        Self {
            _temp: temp,
            engine,
            alpha,
            entry,
            source_id: result["sourceId"].clone(),
            skill_id: state["skills"][0]["id"].clone(),
            target_id,
            state,
        }
    }
    async fn plan(&self, adopt: bool) -> Value {
        run(&self.engine, json!({"action":"plan","skillIds":[self.skill_id],"targetIds":[self.target_id],"adoptExisting":adopt})).await
    }
    async fn apply(&mut self, adopt: bool) {
        self.state = run(&self.engine, json!({"action":"apply_local_source","sourceId":self.source_id,"targetIds":[self.target_id],"expectedRevision":self.state["revision"],"adoptExisting":adopt})).await;
    }
    async fn revoke_source(&mut self) {
        self.state = run(&self.engine, json!({"action":"revoke_local_source","sourceId":self.source_id,"targetIds":[self.target_id],"expectedRevision":self.state["revision"]})).await;
    }
}

#[tokio::test]
async fn adoption_is_opt_in_and_can_promote_a_previously_borrowed_relative_link() {
    let mut f = Fixture::new(true).await;
    assert_eq!(f.plan(false).await["items"][0]["action"], "borrow");
    f.apply(false).await;
    assert_eq!(f.state["bindings"][0]["borrowed"], true);
    f.revoke_source().await;
    assert_eq!(
        fs::read_link(&f.entry).unwrap(),
        Path::new("../source/alpha")
    );
    f.apply(false).await;
    assert_eq!(f.plan(true).await["items"][0]["action"], "adopt");
    assert_eq!(
        fs::read_link(&f.entry).unwrap(),
        Path::new("../source/alpha"),
        "preview cannot change the link"
    );
    let stale_revision = f.state["revision"].clone();
    f.apply(true).await;
    assert_eq!(f.state["bindings"][0]["borrowed"], false);
    assert!(f.state["bindings"][0]["originalLink"].is_null());
    assert_eq!(fs::read_link(&f.entry).unwrap(), f.alpha);
    assert!(f.engine.execute(json!({"action":"apply_local_source","sourceId":f.source_id,"targetIds":[f.target_id],"expectedRevision":stale_revision,"adoptExisting":true})).await.is_err());
    f.state = run(&f.engine, json!({"action":"distribute","skillIds":[f.skill_id],"targetIds":[f.target_id],"expectedRevision":f.state["revision"]})).await;
    f.revoke_source().await;
    assert!(f.entry.is_symlink(), "manual claim still owns the link");
    run(
        &f.engine,
        json!({"action":"revoke","bindingIds":[f.state["bindings"][0]["id"]],"claim":"manual"}),
    )
    .await;
    assert!(!f.entry.is_symlink());
    assert!(
        f.alpha.join("SKILL.md").is_file(),
        "entity must survive last-claim cancellation"
    );
}

#[tokio::test]
async fn direct_adoption_works_for_manual_preset_and_source_distribution() {
    for mode in ["manual", "preset", "source"] {
        let mut f = Fixture::new(false).await;
        let mut preset_id = Value::Null;
        if mode == "preset" {
            f.state = run(
                &f.engine,
                json!({"action":"save_preset","name":"Preset","skillIds":[f.skill_id]}),
            )
            .await;
            preset_id = f.state["presets"][0]["id"].clone();
        }
        let claim = match mode {
            "preset" => format!("preset:{}", preset_id.as_str().unwrap()),
            "source" => format!("source:{}", f.source_id.as_str().unwrap()),
            _ => "manual".into(),
        };
        let plan = run(&f.engine, json!({"action":"plan","claim":claim,"skillIds":[f.skill_id],"targetIds":[f.target_id],"adoptExisting":true})).await;
        assert_eq!(plan["items"][0]["action"], "adopt");
        let action = match mode {
            "preset" => "apply_preset",
            "source" => "apply_local_source",
            _ => "distribute",
        };
        f.state = run(&f.engine, json!({"action":action,"presetId":preset_id,"sourceId":f.source_id,"skillIds":[f.skill_id],"targetIds":[f.target_id],"expectedRevision":plan["revision"],"adoptExisting":true})).await;
        assert_eq!(f.state["bindings"][0]["borrowed"], false);
        assert!(f.state["bindings"][0]["originalLink"].is_null());
        match mode {
            "source" => {
                f.revoke_source().await;
            }
            "preset" => {
                run(&f.engine, json!({"action":"revoke_preset","presetId":preset_id,"targetIds":[f.target_id]})).await;
            }
            _ => {
                run(
                    &f.engine,
                    json!({"action":"revoke","bindingIds":[f.state["bindings"][0]["id"]]}),
                )
                .await;
            }
        }
        assert!(
            !f.entry.is_symlink(),
            "{mode} must remove only the adopted link"
        );
        assert!(f.alpha.join("SKILL.md").is_file());
    }
}

#[tokio::test]
async fn adoption_rejects_real_directories_and_links_changed_after_preview() {
    let mut f = Fixture::new(false).await;
    let plan = f.plan(true).await;
    fs::remove_file(&f.entry).unwrap();
    fs::create_dir(&f.entry).unwrap();
    fs::write(f.entry.join("SKILL.md"), "external content").unwrap();
    assert_eq!(f.plan(true).await["items"][0]["action"], "conflict");
    let request = json!({"action":"apply_local_source","sourceId":f.source_id,"targetIds":[f.target_id],"expectedRevision":plan["revision"],"adoptExisting":true});
    assert!(f.engine.execute(request.clone()).await.is_err());
    assert_eq!(
        fs::read_to_string(f.entry.join("SKILL.md")).unwrap(),
        "external content"
    );
    fs::remove_dir_all(&f.entry).unwrap();
    let other = f.alpha.parent().unwrap().join("other");
    fs::create_dir(&other).unwrap();
    fs::write(other.join("SKILL.md"), "other").unwrap();
    link(&other, &f.entry);
    assert!(f.engine.execute(request).await.is_err());
    assert_eq!(fs::read_link(&f.entry).unwrap(), other);
    fs::remove_file(&f.entry).unwrap();
    link(&f.alpha, &f.entry);
    f.apply(true).await;
    fs::remove_file(&f.entry).unwrap();
    link(&other, &f.entry);
    assert!(f.engine.execute(json!({"action":"revoke_local_source","sourceId":f.source_id,"targetIds":[f.target_id],"expectedRevision":f.state["revision"]})).await.is_err());
    assert_eq!(
        fs::read_link(&f.entry).unwrap(),
        other,
        "cancellation must preserve an externally replaced link"
    );
}
