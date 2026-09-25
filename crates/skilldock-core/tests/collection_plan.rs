use serde_json::{Value, json};
use skilldock_core::Engine;
use std::{
    fs,
    path::{Path, PathBuf},
};

struct Fixture {
    _temp: tempfile::TempDir,
    base: PathBuf,
    engine: Engine,
}
impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let base = fs::canonicalize(temp.path()).unwrap();
        let engine = Engine::new(Some(base.join("config"))).unwrap();
        engine
            .configure(base.join("library").to_str().unwrap())
            .unwrap();
        Self {
            _temp: temp,
            base,
            engine,
        }
    }
    async fn run(&self, request: Value) -> Value {
        self.engine.execute(request).await.unwrap()
    }
    async fn preview(&self, roots: Vec<PathBuf>, mode: &str, adopt: bool) -> Value {
        self.run(json!({"action":"preview_collection","paths":roots,"mode":mode,"adopt":adopt}))
            .await
    }
}
fn skill(path: &Path, name: &str, content: &str) {
    fs::create_dir_all(path).unwrap();
    fs::write(
        path.join("SKILL.md"),
        if name.is_empty() {
            content.into()
        } else {
            format!("---\nname: {name}\ndescription: test\n---\n{content}")
        },
    )
    .unwrap();
}
fn collect(plan: &Value, selected: Vec<PathBuf>, adopt: bool, resolutions: Value) -> Value {
    json!({"action":"collect_skills","paths":plan["paths"],"mode":plan["mode"],
        "adopt":adopt,"expectedRevision":plan["revision"],"fingerprint":plan["fingerprint"],
        "selectedPaths":selected,"resolutions":resolutions})
}

#[tokio::test]
async fn independent_collection_deduplicates_identical_skills_across_different_tool_roots() {
    let f = Fixture::new();
    let first = f.base.join("one");
    let second = f.base.join("two");
    for root in [&first, &second] {
        skill(&root.join("alpha"), "alpha", "same");
    }
    skill(&first.join("beta"), "beta", "b");
    skill(&second.join("gamma"), "gamma", "c");
    let plan = f
        .preview(vec![first.clone(), second.clone()], "individual", true)
        .await;
    assert!(
        plan["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|i| i["status"] == "same")
    );
    let state = f
        .run(collect(
            &plan,
            vec![first.join("alpha"), second.join("alpha")],
            true,
            json!({}),
        ))
        .await;
    assert_eq!(state["skills"].as_array().unwrap().len(), 1);
    assert_eq!(state["bindings"].as_array().unwrap().len(), 2);
    assert_eq!(
        fs::canonicalize(first.join("alpha")).unwrap(),
        fs::canonicalize(second.join("alpha")).unwrap()
    );
    assert!(first.join("beta").is_dir());
    assert!(!first.join("beta").is_symlink());
    assert_eq!(
        fs::read_dir(f.base.join("library/.skilldock/objects"))
            .unwrap()
            .count(),
        1
    );
    for root in [first, second] {
        assert!(!fs::read_dir(root).unwrap().any(|e| {
            e.unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".skilldock-backup-")
        }));
    }
}

#[tokio::test]
async fn conflicts_require_confirmation_and_suffixes_do_not_allow_duplicate_declarations() {
    let f = Fixture::new();
    let a = f.base.join("one/alpha");
    let b = f.base.join("two/alpha");
    skill(&a, "alpha", "one");
    skill(&b, "alpha", "two");
    let plan = f
        .preview(vec![a.clone(), b.clone()], "individual", false)
        .await;
    assert!(
        plan["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|i| i["status"] == "conflict")
    );
    assert!(
        f.engine
            .execute(collect(&plan, vec![a.clone(), b.clone()], false, json!({})))
            .await
            .is_err()
    );
    assert!(f.engine.snapshot().unwrap().skills.is_empty());
    let mut choices = json!({});
    choices[a.to_str().unwrap()] = json!("keep");
    choices[b.to_str().unwrap()] = json!("keep");
    let state = f.run(collect(&plan, vec![a, b], false, choices)).await;
    assert_eq!(state["skills"].as_array().unwrap().len(), 2);
    let ids: Vec<_> = state["skills"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["id"].clone())
        .collect();
    let target = f.base.join("target");
    fs::create_dir(&target).unwrap();
    let state = f
        .run(json!({"action":"add_target","name":"tool","path":target}))
        .await;
    let tid = state["targets"][0]["id"].clone();
    let both = f
        .run(json!({"action":"plan","skillIds":ids,"targetIds":[tid]}))
        .await;
    assert!(
        both["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|i| i["error"].as_str().unwrap().contains("声明同名"))
    );
    f.run(json!({"action":"distribute","skillIds":[ids[0]],"targetIds":[tid],"expectedRevision":state["revision"]})).await;
    let switch = f
        .run(json!({"action":"plan","skillIds":[ids[1]],"targetIds":[tid]}))
        .await;
    assert_eq!(
        switch["items"][0]["error"],
        "目标已使用其他来源，请确认切换来源"
    );
    assert!(!switch["items"][0]["replacement"].is_null());
}

#[tokio::test]
async fn stale_content_and_stale_revision_cannot_be_collected() {
    let f = Fixture::new();
    let path = f.base.join("alpha");
    skill(&path, "alpha", "one");
    let plan = f.preview(vec![path.clone()], "individual", true).await;
    skill(&path, "alpha", "two");
    let error = f
        .engine
        .execute(collect(&plan, vec![path.clone()], true, json!({})))
        .await
        .unwrap_err();
    assert!(error.to_string().contains("来源内容"));
    assert!(!path.is_symlink());
    let plan = f.preview(vec![path.clone()], "individual", true).await;
    f.run(json!({"action":"settings","theme":"dark"})).await;
    let error = f
        .engine
        .execute(collect(&plan, vec![path.clone()], true, json!({})))
        .await
        .unwrap_err();
    assert!(error.to_string().contains("资料库已变化"));
    assert!(!path.is_symlink());
}

#[tokio::test]
async fn package_collection_preserves_shared_resources_and_original_paths() {
    let f = Fixture::new();
    let root = f.base.join("workflow");
    let a = root.join("skills/alpha");
    skill(&a, "alpha", "read ../../shared/data.txt");
    fs::create_dir(root.join("shared")).unwrap();
    fs::write(root.join("shared/data.txt"), "dependency").unwrap();
    let plan = f.preview(vec![root.clone()], "package", false).await;
    f.run(collect(&plan, vec![a.clone()], false, json!({})))
        .await;
    let entity = fs::canonicalize(f.base.join("library/alpha")).unwrap();
    assert_eq!(
        fs::read_to_string(entity.join("../../shared/data.txt")).unwrap(),
        "dependency"
    );
    assert!(!a.is_symlink());
    assert!(root.join("shared/data.txt").exists());
}

#[tokio::test]
async fn skipping_nested_members_cannot_silently_adopt_their_parent() {
    let f = Fixture::new();
    let parent = f.base.join("parent");
    let child = parent.join("child");
    skill(&parent, "parent", "parent");
    skill(&child, "child", "child");
    let plan = f.preview(vec![parent.clone()], "package", true).await;
    let error = f
        .engine
        .execute(collect(&plan, vec![parent.clone()], true, json!({})))
        .await
        .unwrap_err();
    assert!(error.to_string().contains("嵌套成员"));
    assert!(!parent.is_symlink());
    assert!(!child.is_symlink());
}

#[tokio::test]
async fn missing_frontmatter_uses_original_name_instead_of_internal_tree_name() {
    let f = Fixture::new();
    let alpha = f.base.join("alpha");
    let beta = f.base.join("beta");
    skill(&alpha, "", "one");
    skill(&beta, "", "two");
    let plan = f
        .preview(vec![alpha.clone(), beta.clone()], "individual", false)
        .await;
    let state = f
        .run(collect(&plan, vec![alpha, beta], false, json!({})))
        .await;
    let target = f.base.join("target");
    fs::create_dir(&target).unwrap();
    let target_state = f
        .run(json!({"action":"add_target","name":"tool","path":target}))
        .await;
    let ids: Vec<_> = state["skills"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["id"].clone())
        .collect();
    let plan = f
        .run(json!({"action":"plan","skillIds":ids,"targetIds":[target_state["targets"][0]["id"]]}))
        .await;
    assert!(
        plan["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|i| i["error"] == "")
    );
    let other = f.base.join("other/alpha");
    skill(&other, "", "changed");
    let plan = f.preview(vec![other], "individual", false).await;
    assert_eq!(plan["items"][0]["status"], "conflict");
}

#[tokio::test]
async fn new_collection_requires_explicit_legacy_library_migration() {
    let f = Fixture::new();
    let path = f.base.join("alpha");
    skill(&path, "alpha", "one");
    let mut state = f.engine.snapshot().unwrap();
    state.schema_version = 2;
    skilldock_core::files::atomic_json(&Path::new(&state.storage_root).join("state.json"), &state)
        .unwrap();
    let error = f
        .engine
        .execute(json!({"action":"preview_collection","paths":[path]}))
        .await
        .unwrap_err();
    assert!(error.to_string().contains("单份当前内容"));
}
