use serde_json::{Value, json};
use skilldock_core::{Engine, model::Snapshot};
use std::{fs, path::Path, process::Command};

fn write_skill(repo: &Path, name: &str) {
    fs::create_dir_all(repo.join(name)).unwrap();
    fs::write(
        repo.join(name).join("SKILL.md"),
        format!("---\nname: {name}\ndescription: test\n---\n{name}"),
    )
    .unwrap();
}

fn git(repo: &Path, args: &[&str]) {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn commit(repo: &Path) {
    git(repo, &["add", "-A"]);
    git(
        repo,
        &[
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
    );
}

struct Fixture {
    _temp: tempfile::TempDir,
    engine: Engine,
    repo: std::path::PathBuf,
    source: String,
    package: String,
    alpha: String,
    beta: String,
}

impl Fixture {
    async fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let base = fs::canonicalize(temp.path()).unwrap();
        let repo = base.join("repo");
        write_skill(&repo, "alpha");
        write_skill(&repo, "beta");
        git(&repo, &["init", "-q"]);
        commit(&repo);
        let engine = Engine::new(Some(base.join("config"))).unwrap();
        let mut state = engine
            .configure(base.join("library").to_str().unwrap())
            .unwrap();
        state.settings.network_proxy.mode = "direct".into();
        skilldock_core::files::atomic_json(
            &Path::new(&state.storage_root).join("state.json"),
            &state,
        )
        .unwrap();
        let preview = engine
            .execute(json!({"action":"preview_git_package", "url":repo, "reference":"HEAD"}))
            .await
            .unwrap();
        let imported = engine.execute(json!({"action":"import_git_package", "token":preview["token"], "revision":preview["revision"], "autoAdd":true})).await.unwrap();
        let state = engine.snapshot().unwrap();
        let alpha = state
            .skills
            .iter()
            .find(|s| s.name == "alpha")
            .unwrap()
            .clone();
        let beta = state.skills.iter().find(|s| s.name == "beta").unwrap();
        Self {
            _temp: temp,
            engine,
            repo,
            source: alpha.source_id,
            package: imported["packageId"].as_str().unwrap().into(),
            alpha: alpha.id,
            beta: beta.id.clone(),
        }
    }

    async fn save_preset(&self, members: &[String]) {
        let excluded: Vec<_> = [&self.alpha, &self.beta]
            .into_iter()
            .filter(|id| !members.contains(id))
            .collect();
        self.engine.execute(json!({"action":"save_preset", "id":"preset", "name":"Test preset", "skillIds":members, "packages":[{"packageId":self.package,"presetId":"preset", "autoAdd":false, "selectedIds":members, "excludedIds":excluded}]})).await.unwrap();
    }

    async fn remove_upstream_alpha(&self) -> Snapshot {
        fs::remove_dir_all(self.repo.join("alpha")).unwrap();
        commit(&self.repo);
        self.engine.check_source(&self.source, true).await.unwrap()
    }

    async fn uninstall_alpha(&self) -> skilldock_core::error::Result<Value> {
        self.engine
            .execute(json!({"action":"remove_skill", "skillId":self.alpha}))
            .await
    }
}

#[tokio::test]
async fn unreferenced_removed_member_can_be_uninstalled_without_resurrection() {
    let f = Fixture::new().await;
    f.save_preset(&[f.alpha.clone(), f.beta.clone()]).await;
    let missing = f.remove_upstream_alpha().await;
    assert!(
        missing
            .packages
            .iter()
            .find(|p| p.id == f.package)
            .unwrap()
            .missing_member_ids
            .contains(&f.alpha)
    );
    f.save_preset(std::slice::from_ref(&f.beta)).await;
    f.uninstall_alpha().await.unwrap();
    let state = f.engine.snapshot().unwrap();
    assert!(!state.skills.iter().any(|s| s.id == f.alpha));
    assert!(state.skills.iter().any(|s| s.id == f.beta));
    let package = state.packages.iter().find(|p| p.id == f.package).unwrap();
    assert!(!package.member_ids.contains(&f.alpha));
    assert!(!package.missing_member_ids.contains(&f.alpha));
    assert!(
        state
            .preset_packages
            .iter()
            .all(|p| !p.excluded_ids.contains(&f.alpha))
    );
    assert!(
        package
            .scopes
            .iter()
            .any(|s| s.excluded.contains(&"alpha".into()))
    );
    let checked = f.engine.check_source(&f.source, false).await.unwrap();
    let source = checked.sources.iter().find(|s| s.id == f.source).unwrap();
    assert_eq!(source.status, "current");
    assert!(source.error.is_empty());
    write_skill(&f.repo, "alpha");
    commit(&f.repo);
    let restored = f.engine.check_source(&f.source, true).await.unwrap();
    assert!(!restored.skills.iter().any(|s| s.name == "alpha"));
    assert_eq!(restored.presets[0].skill_ids, vec![f.beta]);
}

#[tokio::test]
async fn removed_member_keeps_preset_and_binding_protection() {
    let f = Fixture::new().await;
    f.save_preset(&[f.alpha.clone(), f.beta.clone()]).await;
    let tool = f.repo.parent().unwrap().join("tool");
    let added = f
        .engine
        .execute(json!({"action":"add_target", "name":"Test", "path":tool}))
        .await
        .unwrap();
    let tid = added["targets"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["path"] == tool.display().to_string())
        .unwrap()["id"]
        .clone();
    f.engine.execute(json!({"action":"distribute", "skillIds":[f.alpha], "targetIds":[tid], "expectedRevision":added["revision"]})).await.unwrap();
    f.remove_upstream_alpha().await;
    let before = f.engine.snapshot().unwrap();
    assert!(f.uninstall_alpha().await.is_err());
    assert_eq!(f.engine.snapshot().unwrap().revision, before.revision);
    f.save_preset(std::slice::from_ref(&f.beta)).await;
    assert!(f.uninstall_alpha().await.is_err());
    assert!(tool.join("alpha/SKILL.md").is_file());
    let state = f.engine.snapshot().unwrap();
    let binding = state
        .bindings
        .iter()
        .find(|b| b.skill_id == f.alpha)
        .unwrap();
    f.engine
        .execute(json!({"action":"revoke", "bindingIds":[binding.id]}))
        .await
        .unwrap();
    f.uninstall_alpha().await.unwrap();
    assert!(
        !f.engine
            .snapshot()
            .unwrap()
            .skills
            .iter()
            .any(|s| s.id == f.alpha)
    );
}

#[tokio::test]
async fn present_or_partly_missing_package_member_remains_protected() {
    let f = Fixture::new().await;
    let before = f.engine.snapshot().unwrap();
    assert!(f.uninstall_alpha().await.is_err());
    assert_eq!(f.engine.snapshot().unwrap().revision, before.revision);
    f.remove_upstream_alpha().await;
    let mut state = f.engine.snapshot().unwrap();
    let mut second = state
        .packages
        .iter()
        .find(|p| p.id == f.package)
        .unwrap()
        .clone();
    second.id = "other-package".into();
    second.missing_member_ids.clear();
    state.packages.push(second);
    skilldock_core::files::atomic_json(&Path::new(&state.storage_root).join("state.json"), &state)
        .unwrap();
    assert!(f.uninstall_alpha().await.is_err());
    assert_eq!(f.engine.snapshot().unwrap().revision, state.revision);
}
