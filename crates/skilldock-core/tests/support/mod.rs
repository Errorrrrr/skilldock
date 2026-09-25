use serde_json::{Value, json};
use skilldock_core::{
    Engine, files, id,
    model::{Policy, Skill, Source},
    now,
};
use std::{fs, path::Path};

/// Persist a pre-upgrade reference fixture. New public saves intentionally create
/// managed content, while these tests verify existing external-link behavior.
pub fn legacy_source(engine: &Engine, root: &Path, name: &str, member: &Path) -> Value {
    let mut state = engine.snapshot().unwrap();
    let source_id = id();
    let skill_id = id();
    let (skill_name, description) = files::metadata(member).unwrap();
    let relative = member
        .strip_prefix(root)
        .unwrap()
        .to_string_lossy()
        .replace('\\', "/");
    if state.schema_version >= 3 {
        let storage = Path::new(&state.storage_root);
        let directory = if storage.file_name().is_some_and(|name| name == ".skilldock") {
            storage.parent().unwrap().to_path_buf()
        } else {
            storage.join("skills")
        };
        fs::create_dir_all(&directory).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(member, directory.join(&skill_name)).unwrap();
        #[cfg(windows)]
        std::os::windows::fs::symlink_dir(member, directory.join(&skill_name)).unwrap();
        state
            .library_entries
            .insert(skill_id.clone(), skill_name.clone());
    }
    state.skills.push(Skill {
        id: skill_id.clone(),
        name: skill_name,
        description,
        source_id: source_id.clone(),
        external_path: Some(member.display().to_string()),
        bundle_digest: String::new(),
        relative_path: relative,
        version: "跟随本地内容".into(),
        installed_at: now(),
    });
    state.sources.push(Source {
        id: source_id.clone(),
        name: name.into(),
        kind: "local_reference".into(),
        path: root.display().to_string(),
        url: String::new(),
        scan_subdir: String::new(),
        reference: String::new(),
        version: "跟随本地内容".into(),
        policy: Policy::default(),
        last_checked: String::new(),
        next_check: String::new(),
        status: "local_reference".into(),
        error: String::new(),
        updates_removed: None,
        local_member_ids: Some(vec![skill_id]),
    });
    state.revision += 1;
    files::atomic_json(&Path::new(&state.storage_root).join("state.json"), &state).unwrap();
    json!({"snapshot": state, "sourceId": source_id})
}
