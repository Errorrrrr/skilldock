use super::*;

fn setup() -> (tempfile::TempDir, Engine, PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let base = fs::canonicalize(temp.path()).unwrap();
    let engine = Engine::new(Some(base.join("config"))).unwrap();
    let state = engine
        .configure(base.join("library").to_str().unwrap())
        .unwrap();
    assert_eq!(state.settings.backup_retention, 0);
    engine
        .transact("test", "isolate configured scan roots", |state, _, _| {
            state.settings.agent_profiles.clear();
            Ok(())
        })
        .unwrap();
    let source = base.join("tools");
    fs::create_dir_all(&source).unwrap();
    (temp, engine, source)
}

fn skill(source: &Path, name: &str) -> PathBuf {
    let path = source.join(name);
    fs::create_dir_all(&path).unwrap();
    fs::write(path.join("SKILL.md"), format!("# {name}\nOriginal content")).unwrap();
    path
}

fn temporary_originals(source: &Path) -> Vec<PathBuf> {
    fs::read_dir(source)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with(".skilldock-backup-")
        })
        .collect()
}

#[test]
fn zero_retention_commits_links_and_cleans_originals_with_dependencies() {
    let (_temp, engine, source) = setup();
    let original = skill(&source, "alpha");
    fs::create_dir_all(original.join("node_modules/helper")).unwrap();
    fs::write(original.join("node_modules/helper/index.js"), "dependency").unwrap();

    let state = engine.import_folder(&original, vec![], true, None).unwrap();

    assert!(fs::read_link(&original).is_ok());
    assert_eq!(
        fs::read_to_string(original.join("node_modules/helper/index.js")).unwrap(),
        "dependency"
    );
    assert_eq!(
        fs::canonicalize(&original).unwrap(),
        fs::canonicalize(skill_path(Path::new(&state.storage_root), &state.skills[0])).unwrap()
    );
    assert!(temporary_originals(&source).is_empty());
    let backups = engine.list_backups().unwrap();
    assert_eq!(backups.len(), 1);
    assert_eq!(backups[0].status, "expired");
}

#[test]
fn zero_retention_preserves_original_when_library_staging_fails() {
    let (_temp, engine, source) = setup();
    let original = skill(&source, "alpha");
    let root = engine.root().unwrap().unwrap();
    // A file in place of the destination directory deterministically prevents
    // staging/copying without relying on platform-specific permission behavior.
    fs::remove_dir(root.join("objects")).unwrap();
    fs::write(root.join("objects"), "occupied").unwrap();

    assert!(engine.import_folder(&original, vec![], true, None).is_err());

    assert!(fs::symlink_metadata(&original).unwrap().is_dir());
    assert!(
        fs::read_to_string(original.join("SKILL.md"))
            .unwrap()
            .contains("Original content")
    );
    assert!(temporary_originals(&source).is_empty());
    assert!(engine.snapshot().unwrap().skills.is_empty());
}

#[test]
fn zero_retention_rolls_back_original_when_link_creation_fails() {
    let (_temp, engine, source) = setup();
    let original = skill(&source, "alpha");
    let backup = source.join(format!(".skilldock-backup-{}", id()));
    let digest = files::manifest_digest(&original).unwrap();
    let result = engine.transact("import", "simulate link failure", |_, _, changes| {
        changes.push(Change {
            path: original.clone(),
            before: None,
            // Embedded NUL forces the OS link operation to fail after rename.
            after: Some(PathBuf::from("invalid\0link-target")),
            backup: Some(backup.clone()),
            restore: false,
            backup_digest: Some(digest.clone()),
        });
        Ok(())
    });

    assert!(result.is_err());
    assert!(fs::symlink_metadata(&original).unwrap().is_dir());
    assert_eq!(files::manifest_digest(&original).unwrap(), digest);
    assert!(!backup.exists());
    assert!(
        !engine
            .snapshot()
            .unwrap()
            .tasks
            .iter()
            .any(|task| task.status == "needsRecovery")
    );
}

#[test]
fn zero_retention_preserves_changed_original_when_validation_fails() {
    let (_temp, engine, source) = setup();
    let original = skill(&source, "alpha");
    let digest = files::manifest_digest(&original).unwrap();
    let backup = source.join(format!(".skilldock-backup-{}", id()));
    let result = engine.transact(
        "import",
        "simulate concurrent source edit",
        |_, root, changes| {
            fs::write(original.join("SKILL.md"), "Edited after copying").unwrap();
            changes.push(Change {
                path: original.clone(),
                before: None,
                after: Some(root.join("objects/uncommitted/tree")),
                backup: Some(backup.clone()),
                restore: false,
                backup_digest: Some(digest),
            });
            Ok(())
        },
    );

    assert!(result.is_err());
    assert!(fs::symlink_metadata(&original).unwrap().is_dir());
    assert_eq!(
        fs::read_to_string(original.join("SKILL.md")).unwrap(),
        "Edited after copying"
    );
    assert!(!backup.exists());
}

#[test]
fn zero_retention_recovers_an_interrupted_original_rename() {
    let (_temp, engine, source) = setup();
    let original = skill(&source, "alpha");
    let backup = source.join(format!(".skilldock-backup-{}", id()));
    let state = engine.snapshot().unwrap();
    let root = Path::new(&state.storage_root);
    let journal = Journal {
        id: id(),
        status: "running".into(),
        changes: vec![Change {
            path: original.clone(),
            before: None,
            after: Some(root.join("objects/uncommitted/tree")),
            backup: Some(backup.clone()),
            restore: false,
            backup_digest: Some(files::manifest_digest(&original).unwrap()),
        }],
        before: state.clone(),
        after: state.clone(),
        error: "simulated interruption after rename".into(),
        pruning: None,
        object_cleanup: None,
    };
    files::atomic_json(
        &root
            .join("transactions")
            .join(format!("{}.json", journal.id)),
        &journal,
    )
    .unwrap();
    fs::rename(&original, &backup).unwrap();

    assert!(
        engine
            .snapshot()
            .unwrap()
            .tasks
            .iter()
            .any(|task| task.status == "needsRecovery")
    );
    engine.recover(&journal.id).unwrap();

    assert!(fs::symlink_metadata(&original).unwrap().is_dir());
    assert!(original.join("SKILL.md").is_file());
    assert!(!backup.exists());
}

#[test]
fn explicit_retention_survives_reopening_and_keeps_historical_restore() {
    let (_temp, engine, source) = setup();
    engine
        .execute_local("settings", &json!({"backupRetention": 3}))
        .unwrap();
    let original = skill(&source, "alpha");
    engine.import_folder(&original, vec![], true, None).unwrap();
    let backup_id = engine.list_backups().unwrap()[0].id.clone();

    let reopened = Engine::new(Some(engine.config_dir.clone())).unwrap();
    assert_eq!(reopened.snapshot().unwrap().settings.backup_retention, 3);
    assert_eq!(reopened.list_backups().unwrap()[0].status, "available");
    assert_eq!(temporary_originals(&source).len(), 1);
    reopened.restore_backup(&backup_id).unwrap();

    assert!(fs::symlink_metadata(&original).unwrap().is_dir());
    assert!(original.join("SKILL.md").is_file());
    assert!(temporary_originals(&source).is_empty());
}

#[test]
fn zero_retention_keeps_modified_backup_or_divergent_library_copy() {
    for modify_backup in [true, false] {
        let (_temp, engine, source) = setup();
        engine
            .execute_local("settings", &json!({"backupRetention": 1}))
            .unwrap();
        let original = skill(&source, "alpha");
        engine.import_folder(&original, vec![], true, None).unwrap();
        let backup = temporary_originals(&source).remove(0);
        let changed = if modify_backup {
            backup.clone()
        } else {
            original.clone()
        };
        fs::write(changed.join("external.txt"), "must survive").unwrap();

        let state = engine
            .execute_local("settings", &json!({"backupRetention": 0}))
            .unwrap();

        assert!(backup.join("SKILL.md").is_file());
        assert_eq!(
            fs::read_to_string(changed.join("external.txt")).unwrap(),
            "must survive"
        );
        assert!(fs::read_link(&original).is_ok());
        assert!(
            state["tasks"]
                .as_array()
                .unwrap()
                .iter()
                .any(|task| { task["kind"] == "backup_cleanup" && task["status"] == "failed" })
        );
    }
}
