use super::*;

fn setup() -> (tempfile::TempDir, Engine, PathBuf, String) {
    let temp = tempfile::tempdir().unwrap();
    let base = fs::canonicalize(temp.path()).unwrap();
    let engine = Engine::new(Some(base.join("config"))).unwrap();
    engine
        .configure_legacy_fixture(base.join("library").to_str().unwrap())
        .unwrap();
    let mut isolated = engine.snapshot().unwrap();
    isolated.settings.agent_profiles.clear();
    files::atomic_json(&base.join("library/state.json"), &isolated).unwrap();
    let original = base.join("source");
    fs::create_dir_all(original.join("node_modules/pkg")).unwrap();
    fs::write(original.join("SKILL.md"), "# Test").unwrap();
    fs::write(
        original.join("node_modules/pkg/runtime.js"),
        "working dependency",
    )
    .unwrap();
    let state = engine.import_folder(&original, vec![], true, None).unwrap();
    let backup = state.tasks.last().unwrap().id.clone();
    (temp, engine, original, backup)
}
fn choices(plan: &RestorePreview) -> Vec<CleanupChoice> {
    plan.items
        .iter()
        .filter(|i| i.status == "ready")
        .map(|i| CleanupChoice {
            digest: i.digest.clone(),
            fingerprint: i.fingerprint.clone(),
        })
        .collect()
}
fn save(engine: &Engine, journal: &Journal) {
    files::atomic_json(
        &engine
            .root()
            .unwrap()
            .unwrap()
            .join("transactions")
            .join(format!("{}.json", journal.id)),
        journal,
    )
    .unwrap();
}
// Persist the state left by a committed restore whose cleanup has not run yet.
fn scheduled() -> (tempfile::TempDir, Engine, PathBuf, Journal) {
    let (temp, engine, original, backup) = setup();
    let before = engine.snapshot().unwrap();
    let root = engine.root().unwrap().unwrap();
    let mut after = before.clone();
    let mut changes = vec![];
    engine
        .prepare_restore(&mut after, &root, &mut changes, &backup)
        .unwrap();
    let plan = engine
        .plan_restore_cleanup(&root, &before, &after, &changes)
        .unwrap();
    let cleanup = engine
        .prepare_object_cleanup(&root, &before, &changes, &plan, choices(&plan))
        .unwrap();
    engine.restore_backup(&backup).unwrap();
    let mut state = engine.snapshot().unwrap();
    let prior = state.clone();
    let job_id = id();
    state.revision += 1;
    state.tasks.push(Task {
        id: job_id.clone(),
        kind: "object_cleanup".into(),
        title: "test".into(),
        status: "success".into(),
        message: String::new(),
        created_at: now(),
    });
    let journal = Journal {
        id: job_id,
        status: "committed".into(),
        changes: vec![],
        before: prior,
        after: state.clone(),
        error: String::new(),
        pruning: None,
        object_cleanup: Some(cleanup),
    };
    save(&engine, &journal);
    files::atomic_json(&root.join("state.json"), &state).unwrap();
    (temp, engine, original, journal)
}
fn stage(engine: &Engine, journal: &mut Journal, phase: &str) -> PathBuf {
    let root = engine.root().unwrap().unwrap();
    let work = &mut journal.object_cleanup.as_mut().unwrap().items[0];
    let object = Engine::object_path(&root, &work.choice.digest).unwrap();
    work.inventory = Some(files::tree_manifest(&object, false).unwrap());
    let staged = Engine::cleanup_staging(&root, &journal.id)
        .unwrap()
        .join(&work.choice.digest);
    fs::rename(object, &staged).unwrap();
    work.phase = phase.into();
    work.status = "failed".into();
    work.reason = "interrupted".into();
    save(engine, journal);
    staged
}

#[test]
fn restore_and_cleanup_removes_only_confirmed_versions_and_preserves_original_dependencies() {
    let (temp, engine, original, backup) = setup();
    let root = engine.root().unwrap().unwrap();
    // Two intermediate updates are retained in journals; the library versions
    // are independent of the old source directory that will be restored.
    for n in 1..=2 {
        let next = temp.path().join(format!("version-{n}"));
        fs::create_dir(&next).unwrap();
        fs::write(next.join("SKILL.md"), format!("# Version {n}")).unwrap();
        let digest = files::snapshot_tree(&root, &next).unwrap();
        engine
            .transact("update", "test update", |s, _, _| {
                s.skills[0].bundle_digest = digest.clone();
                s.sources[0].version = digest;
                Ok(())
            })
            .unwrap();
    }
    let plan = engine.preview_restore(&backup).unwrap();
    assert_eq!(choices(&plan).len(), 3);
    let selected = choices(&plan).into_iter().take(2).collect();
    let result = engine
        .restore_backup_cleanup(&backup, Some(plan.revision), selected)
        .unwrap();
    assert!(result.skills.is_empty());
    assert!(
        !fs::symlink_metadata(&original)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(
        fs::read_to_string(original.join("node_modules/pkg/runtime.js")).unwrap(),
        "working dependency"
    );
    assert_eq!(
        plan.items
            .iter()
            .filter(|i| Path::new(&i.path).exists())
            .count(),
        1
    );
    let reports = engine.list_object_cleanups().unwrap();
    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0].status, "complete");
    assert!(reports[0].items.iter().all(|i| i.status == "removed"));
}

#[test]
fn history_cleanup_revalidates_revision_content_and_selection() {
    let (_temp, engine, original, backup) = setup();
    engine.restore_backup(&backup).unwrap();
    let plan = engine.preview_object_cleanup().unwrap();
    let object = Path::new(&plan.items[0].path);
    let mut forged = choices(&plan);
    forged[0].fingerprint = "forged".into();
    assert!(engine.cleanup_objects(plan.revision, forged).is_err());
    fs::write(object.join("tree/user-note.txt"), "keep this edit").unwrap();
    assert!(
        engine
            .cleanup_objects(plan.revision, choices(&plan))
            .is_err()
    );
    assert!(object.join("tree/user-note.txt").exists());
    fs::remove_file(object.join("tree/user-note.txt")).unwrap();
    engine
        .transact("test", "state change", |_, _, _| Ok(()))
        .unwrap();
    assert!(
        engine
            .cleanup_objects(plan.revision, choices(&plan))
            .is_err()
    );
    let plan = engine.preview_object_cleanup().unwrap();
    engine
        .cleanup_objects(plan.revision, choices(&plan))
        .unwrap();
    assert!(!object.exists());
    assert!(original.join("SKILL.md").is_file());
}

#[test]
fn cleanup_cannot_run_without_transaction_commit() {
    let (_temp, engine, _, mut journal) = scheduled();
    let mut state = engine.snapshot().unwrap();
    state.tasks.retain(|t| t.id != journal.id);
    journal.status = "running".into();
    let object = Engine::object_path(
        &engine.root().unwrap().unwrap(),
        &journal.object_cleanup.as_ref().unwrap().items[0]
            .choice
            .digest,
    )
    .unwrap();
    assert!(
        engine
            .run_object_cleanup(&engine.root().unwrap().unwrap(), &state, &mut journal)
            .is_err()
    );
    assert!(object.join("tree/SKILL.md").exists());
}

#[test]
fn retry_keeps_new_reference_or_changed_content() {
    for changed in [true, false] {
        let (_temp, engine, _, journal) = scheduled();
        let root = engine.root().unwrap().unwrap();
        let digest = &journal.object_cleanup.as_ref().unwrap().items[0]
            .choice
            .digest;
        let object = root.join("objects").join(digest);
        if changed {
            fs::write(object.join("tree/notes"), "edit").unwrap();
        } else {
            engine
                .transact("test", "new reference", |s, _, _| {
                    let mut skill =
                        journal
                            .before
                            .skills
                            .first()
                            .cloned()
                            .unwrap_or_else(|| Skill {
                                external_path: None,
                                id: id(),
                                name: "new reference".into(),
                                description: String::new(),
                                source_id: id(),
                                bundle_digest: digest.clone(),
                                relative_path: String::new(),
                                version: String::new(),
                                installed_at: now(),
                            });
                    skill.bundle_digest = digest.clone();
                    s.skills.push(skill);
                    Ok(())
                })
                .unwrap();
        }
        engine.retry_object_cleanup(&journal.id).unwrap();
        assert!(object.exists());
        assert_eq!(
            engine.list_object_cleanups().unwrap()[0].items[0].status,
            "retained"
        );
    }
}

#[test]
fn partial_cleanup_retries_without_touching_reinstalled_object() {
    let (_temp, engine, original, mut journal) = scheduled();
    let root = engine.root().unwrap().unwrap();
    let staged = stage(&engine, &mut journal, "deleting");
    fs::remove_file(staged.join("tree/SKILL.md")).unwrap();
    let new_digest = files::snapshot_tree(&root, &original).unwrap();
    engine.retry_object_cleanup(&journal.id).unwrap();
    assert!(!staged.exists());
    assert!(
        root.join("objects")
            .join(new_digest)
            .join("tree/SKILL.md")
            .is_file()
    );
    assert_eq!(
        engine.list_object_cleanups().unwrap()[0].items[0].status,
        "removed"
    );
}

#[test]
fn retry_keeps_new_files_in_partial_staging_tree() {
    let (_temp, engine, _, mut journal) = scheduled();
    let staged = stage(&engine, &mut journal, "deleting");
    fs::remove_file(staged.join("tree/SKILL.md")).unwrap();
    fs::write(staged.join("tree/added-after-interruption"), "keep").unwrap();
    engine.retry_object_cleanup(&journal.id).unwrap();
    assert!(staged.join("tree/added-after-interruption").exists());
    let reports = engine.list_object_cleanups().unwrap();
    assert_eq!(reports[0].items[0].status, "retained");
    assert_eq!(reports[0].items[0].path, staged.display().to_string());
}

#[test]
#[cfg(unix)]
fn retry_protects_external_links_into_staged_object() {
    let (temp, engine, _, mut journal) = scheduled();
    let staged = stage(&engine, &mut journal, "staging");
    std::os::unix::fs::symlink(
        staged.join("tree/SKILL.md"),
        temp.path().join("manual-link"),
    )
    .unwrap();
    engine.retry_object_cleanup(&journal.id).unwrap();
    assert!(staged.join("tree/SKILL.md").exists());
    assert_eq!(
        engine.list_object_cleanups().unwrap()[0].items[0].status,
        "retained"
    );
}

#[test]
fn ambiguous_staging_crash_never_deletes_new_same_digest_object() {
    let (_temp, engine, original, mut journal) = scheduled();
    let root = engine.root().unwrap().unwrap();
    let staged = stage(&engine, &mut journal, "staging");
    let moved = staged.with_extension("moved");
    fs::rename(&staged, &moved).unwrap();
    let digest = files::snapshot_tree(&root, &original).unwrap();
    engine.retry_object_cleanup(&journal.id).unwrap();
    assert!(
        root.join("objects")
            .join(digest)
            .join("tree/SKILL.md")
            .exists()
    );
    assert!(moved.join("tree/SKILL.md").exists());
    assert_eq!(
        engine.list_object_cleanups().unwrap()[0].items[0].status,
        "retained"
    );
}

#[test]
#[cfg(unix)]
fn permission_failure_is_retryable_and_does_not_undo_restore() {
    use std::os::unix::fs::PermissionsExt;
    let (_temp, engine, original, mut journal) = scheduled();
    let staged = stage(&engine, &mut journal, "deleting");
    let protected = staged.join("tree/node_modules/pkg");
    fs::set_permissions(&protected, fs::Permissions::from_mode(0o555)).unwrap();
    let first = engine.retry_object_cleanup(&journal.id);
    fs::set_permissions(&protected, fs::Permissions::from_mode(0o755)).unwrap();
    first.unwrap();
    assert_eq!(engine.list_object_cleanups().unwrap()[0].status, "pending");
    assert!(original.join("node_modules/pkg/runtime.js").exists());
    assert!(
        !fs::symlink_metadata(&original)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    engine.retry_object_cleanup(&journal.id).unwrap();
    assert!(!staged.exists());
    assert_eq!(engine.list_object_cleanups().unwrap()[0].status, "complete");
}

#[test]
fn pending_cleanup_survives_library_migration() {
    let (_temp, engine, _, mut journal) = scheduled();
    let staged = stage(&engine, &mut journal, "deleting");
    fs::remove_file(staged.join("tree/SKILL.md")).unwrap();
    let destination = tempfile::tempdir().unwrap();
    let new_root = destination.path().join("new-library");
    engine.migrate_storage(new_root.to_str().unwrap()).unwrap();
    engine.retry_object_cleanup(&journal.id).unwrap();
    assert_eq!(engine.list_object_cleanups().unwrap()[0].status, "complete");
    assert!(
        !engine
            .root()
            .unwrap()
            .unwrap()
            .join(".object-cleanup")
            .join(journal.id)
            .exists()
    );
}

#[tokio::test]
async fn execute_api_accepts_explicit_choices_and_lists_result() {
    let (_temp, engine, original, backup) = setup();
    let value = engine
        .execute(json!({ "action": "preview_restore", "backupId": backup }))
        .await
        .unwrap();
    let plan = engine.preview_restore(&backup).unwrap();
    engine
        .execute(json!({ "action": "restore_backup", "backupId": backup,
        "expectedRevision": value["revision"], "cleanupItems": choices(&plan) }))
        .await
        .unwrap();
    let result = engine
        .execute(json!({ "action": "list_object_cleanups" }))
        .await
        .unwrap();
    assert_eq!(result[0]["items"][0]["status"], "removed");
    assert!(original.join("SKILL.md").exists());
    assert!(!Path::new(&plan.items[0].path).exists());
}
