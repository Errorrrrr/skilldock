use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

struct Fixture {
    _temp: tempfile::TempDir,
    base: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let base = fs::canonicalize(temp.path()).unwrap();
        let fixture = Self { _temp: temp, base };
        let library = fixture.base.join("library");
        fixture.ok(&["init", library.to_str().unwrap()]);
        let path = library.join(".skilldock/state.json");
        let mut state: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        // Local Git fixtures must not inherit the machine's system proxy policy.
        state["settings"]["networkProxy"] = json!({"mode":"direct","url":""});
        // Keep all destinations inside the fixture, independent of installed tools.
        state["settings"]["agentProfiles"] = json!([
            {"id":"codex","name":"Codex","userPaths":[fixture.base.join("codex")],"projectPaths":[]},
            {"id":"workbuddy","name":"WorkBuddy","userPaths":[fixture.base.join("workbuddy"),fixture.base.join("codebuddy")],"projectPaths":[]}
        ]);
        fs::write(path, serde_json::to_vec(&state).unwrap()).unwrap();
        fixture
    }

    fn run(&self, args: &[&str]) -> (bool, Value) {
        let output = Command::new(env!("CARGO_BIN_EXE_skilldock"))
            .arg("--config-dir")
            .arg(self.base.join("config"))
            .arg("--json")
            .args(args)
            .current_dir(&self.base)
            .output()
            .unwrap();
        let data = if output.stdout.is_empty() {
            &output.stderr
        } else {
            &output.stdout
        };
        let value = serde_json::from_slice(data).unwrap_or_else(|e| {
            panic!(
                "invalid output: {e}; stdout={} stderr={}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            )
        });
        (output.status.success(), value)
    }

    fn ok(&self, args: &[&str]) -> Value {
        let (success, result) = self.run(args);
        assert!(success, "{result}");
        result
    }

    fn skill(&self, relative: &str, name: &str, content: &str) -> PathBuf {
        let path = self.base.join(relative);
        fs::create_dir_all(&path).unwrap();
        fs::write(
            path.join("SKILL.md"),
            format!("---\nname: {name}\ndescription: Test\n---\n{content}\n"),
        )
        .unwrap();
        path
    }
}

#[test]
fn local_install_shares_one_entity_and_repeated_install_reuses_bindings() {
    let f = Fixture::new();
    let source = f.skill("package/alpha", "alpha", "Read ../shared.txt");
    fs::write(f.base.join("package/shared.txt"), "shared dependency").unwrap();
    let args = [
        "install",
        "./package",
        "--to",
        "codex",
        "--to",
        "workbuddy",
        "--to",
        "codex",
    ];
    let first = f.ok(&args);
    assert_eq!(first["status"], "succeeded");
    assert_eq!(first["targets"].as_array().unwrap().len(), 2);
    let codex = fs::canonicalize(f.base.join("codex/alpha")).unwrap();
    let workbuddy = fs::canonicalize(f.base.join("workbuddy/alpha")).unwrap();
    assert_eq!(codex, workbuddy);
    assert_ne!(codex, source);
    assert!(source.join("SKILL.md").is_file());
    assert_eq!(
        fs::read_to_string(codex.parent().unwrap().join("shared.txt")).unwrap(),
        "shared dependency"
    );
    let before = f.ok(&["list"]);
    let second = f.ok(&args);
    let after = f.ok(&["list"]);
    assert_eq!(first["skillIds"], second["skillIds"]);
    assert_eq!(before["bindings"], after["bindings"]);
    assert_eq!(after["skills"].as_array().unwrap().len(), 1);
}

#[test]
fn invalid_target_and_ambiguous_tool_fail_before_import() {
    let f = Fixture::new();
    f.skill("source", "alpha", "test");
    for path in ["workbuddy", "codebuddy"] {
        fs::create_dir(f.base.join(path)).unwrap();
    }
    for selector in ["typo", "workbuddy"] {
        let (success, result) = f.run(&["install", "./source", "--to", selector]);
        assert!(!success);
        assert_eq!(result["stage"], "validate");
        assert_eq!(result["imported"], false);
    }
    assert!(f.ok(&["list"])["skills"].as_array().unwrap().is_empty());
}

#[test]
fn partial_failure_preserves_external_directory_and_other_target_succeeds() {
    let f = Fixture::new();
    f.skill("source/alpha", "alpha", "managed");
    let external = f.skill("codex/alpha", "alpha", "external");
    let (success, report) = f.run(&["install", "./source", "--to", "codex", "--to", "workbuddy"]);
    assert!(!success);
    assert_eq!(report["status"], "partial");
    assert_eq!(report["imported"], true);
    assert_eq!(report["targets"][0]["status"], "failed");
    assert_eq!(report["targets"][1]["status"], "succeeded");
    assert!(
        fs::read_to_string(external.join("SKILL.md"))
            .unwrap()
            .contains("external")
    );
    assert!(
        !fs::symlink_metadata(&external)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert!(
        fs::symlink_metadata(f.base.join("workbuddy/alpha"))
            .unwrap()
            .file_type()
            .is_symlink()
    );
}

#[test]
fn selection_limits_distribution_and_keeps_whole_package() {
    let f = Fixture::new();
    f.skill("package/alpha", "alpha", "alpha");
    f.skill("package/beta", "beta", "beta");
    let report = f.ok(&[
        "install", "package", "--from", "local", "--select", "alpha", "--to", "codex",
    ]);
    assert_eq!(report["skillIds"].as_array().unwrap().len(), 1);
    assert!(!f.base.join("codex/beta").exists());
    let entity = fs::canonicalize(f.base.join("codex/alpha")).unwrap();
    assert!(entity.parent().unwrap().join("beta/SKILL.md").exists());
}

#[test]
fn explicit_project_target_does_not_install_to_user_directory() {
    let f = Fixture::new();
    f.skill("source", "alpha", "test");
    let target = f.base.join("project/.agents/skills");
    let state = f.ok(&[
        "target",
        "Project",
        target.to_str().unwrap(),
        "--tool",
        "codex",
    ]);
    let id = state["targets"][0]["id"].as_str().unwrap();
    f.ok(&["install", "./source", "--to", id]);
    assert!(target.join("alpha/SKILL.md").exists());
    assert!(!f.base.join("codex").exists());
}

#[test]
fn invalid_source_options_and_missing_members_do_not_import() {
    let f = Fixture::new();
    f.skill("source", "alpha", "test");
    for args in [
        vec![
            "install", "./source", "--from", "local", "--site", "clawhub",
        ],
        vec!["install", "./source", "--reference", "main"],
        vec![
            "install",
            "owner/slug",
            "--site",
            "clawhub",
            "--select",
            "alpha",
        ],
        vec!["install", "./source", "--select", "../outside"],
        vec!["install", "./source", "--select", "missing"],
    ] {
        let (success, report) = f.run(&args);
        assert!(!success, "{report}");
    }
    assert!(f.ok(&["list"])["skills"].as_array().unwrap().is_empty());
}

#[test]
fn import_only_and_bundled_agent_skill_are_supported() {
    let f = Fixture::new();
    let skill = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../integrations/skilldock");
    let report = f.ok(&["install", skill.to_str().unwrap()]);
    assert_eq!(report["status"], "succeeded");
    assert!(report["targets"].as_array().unwrap().is_empty());
    let state = f.ok(&["list"]);
    assert_eq!(state["skills"][0]["name"], "skilldock");
    assert!(state["targets"].as_array().unwrap().is_empty());
}

#[test]
fn git_install_resolves_selected_members_and_reuses_canonical_ids() {
    let f = Fixture::new();
    f.skill("repository/skills/alpha", "alpha", "Use ../../shared.txt");
    f.skill("repository/skills/beta", "beta", "beta");
    let repo = f.base.join("repository");
    fs::write(repo.join("shared.txt"), "git dependency").unwrap();
    for args in [
        vec!["init", "--quiet"],
        vec!["add", "."],
        vec![
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "--quiet",
            "-m",
            "fixture",
        ],
    ] {
        let output = Command::new("git")
            .args(args)
            .current_dir(&repo)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let args = [
        "install",
        repo.to_str().unwrap(),
        "--from",
        "git",
        "--subdir",
        "skills",
        "--select",
        "skills/alpha",
        "--to",
        "codex",
    ];
    let first = f.ok(&args);
    assert_eq!(first["skillIds"].as_array().unwrap().len(), 1);
    let installed = fs::canonicalize(f.base.join("codex/alpha")).unwrap();
    assert_eq!(
        fs::read_to_string(
            installed
                .parent()
                .unwrap()
                .parent()
                .unwrap()
                .join("shared.txt")
        )
        .unwrap(),
        "git dependency"
    );
    assert!(!f.base.join("codex/beta").exists());
    let second = f.ok(&args);
    assert_eq!(first["skillIds"], second["skillIds"]);
    let state = f.ok(&["list"]);
    assert_eq!(state["bindings"].as_array().unwrap().len(), 1);
}

#[test]
fn same_name_from_another_local_source_does_not_replace_managed_content() {
    let f = Fixture::new();
    f.skill("first", "alpha", "first content");
    f.skill("second", "alpha", "different content");
    f.ok(&["install", "./first", "--to", "codex"]);
    let before = fs::read_link(f.base.join("codex/alpha")).unwrap();
    let (success, report) = f.run(&["install", "./second", "--to", "codex"]);
    assert!(!success, "{report}");
    assert_eq!(before, fs::read_link(f.base.join("codex/alpha")).unwrap());
    assert!(
        fs::read_to_string(f.base.join("codex/alpha/SKILL.md"))
            .unwrap()
            .contains("first content")
    );
}

#[test]
fn selecting_another_member_preserves_previous_source_members_and_claims() {
    let f = Fixture::new();
    f.skill("package/alpha", "alpha", "alpha");
    f.skill("package/beta", "beta", "beta");
    let first = f.ok(&["install", "./package", "--select", "alpha", "--to", "codex"]);
    let state = f.ok(&["list"]);
    let request = json!({"action":"apply_local_source","sourceId":state["sources"][0]["id"],
        "targetIds":[first["targets"][0]["id"]],"expectedRevision":state["revision"]})
    .to_string();
    f.ok(&["exec", &request]);
    let second = f.ok(&["install", "./package", "--select", "beta", "--to", "codex"]);
    assert_eq!(second["skillIds"].as_array().unwrap().len(), 1);
    assert_ne!(first["skillIds"], second["skillIds"]);
    let state = f.ok(&["list"]);
    assert_eq!(
        state["sources"][0]["localMemberIds"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let alpha = state["bindings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["skillId"] == first["skillIds"][0])
        .unwrap();
    assert!(
        alpha["claims"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c.as_str().unwrap().starts_with("source:"))
    );
    assert!(f.base.join("codex/alpha/SKILL.md").is_file());
}
