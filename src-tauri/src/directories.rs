use std::{fs, path::PathBuf};
#[cfg(not(target_os = "macos"))]
use tauri_plugin_opener::OpenerExt;

#[derive(Debug, PartialEq)]
pub(crate) enum DirectoryAction {
    Open(PathBuf),
    RevealEntry(PathBuf),
}

pub(crate) fn resolve(path: &str, reveal_link: bool) -> Result<DirectoryAction, String> {
    let path = skilldock_core::files::absolute(path).map_err(|e| e.to_string())?;
    // Resolve the actual directory only for the entity action. The source action
    // must keep the original link entry, including a link enclosing a nested Skill.
    let metadata = fs::symlink_metadata(&path)
        .map_err(|e| format!("目录不可用（可能已迁移或删除）：{}；{e}", path.display()))?;
    if reveal_link && metadata.file_type().is_symlink() {
        return Ok(DirectoryAction::RevealEntry(path));
    }
    let actual =
        fs::canonicalize(&path).map_err(|e| format!("无法解析目录 {}：{e}", path.display()))?;
    if !actual.is_dir() {
        return Err("只能打开文件夹，不能打开文件或执行脚本".into());
    }
    if reveal_link {
        for parent in path.ancestors().skip(1) {
            if fs::symlink_metadata(parent).is_ok_and(|m| m.file_type().is_symlink())
                && parent.join("SKILL.md").is_file()
            {
                return Ok(DirectoryAction::RevealEntry(parent.to_path_buf()));
            }
        }
    }
    // Finder packages must be located rather than launched by file associations.
    #[cfg(target_os = "macos")]
    if actual
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| {
            [
                "app", "workflow", "scptd", "prefpane", "saver", "bundle", "plugin", "xpc", "appex",
            ]
            .contains(&value.to_ascii_lowercase().as_str())
        })
    {
        return Ok(DirectoryAction::RevealEntry(actual));
    }
    Ok(DirectoryAction::Open(actual))
}

#[tauri::command]
pub(crate) async fn open_directory(
    app: tauri::AppHandle,
    path: String,
    reveal_link: bool,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || match resolve(&path, reveal_link)? {
        DirectoryAction::Open(path) => open_folder(&app, path),
        DirectoryAction::RevealEntry(path) => reveal_entry(&app, path),
    })
    .await
    .map_err(|e| e.to_string())?
}

fn open_folder(app: &tauri::AppHandle, path: PathBuf) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let _ = app;
        let status = std::process::Command::new("/usr/bin/open")
            .args(["-a", "/System/Library/CoreServices/Finder.app", "--"])
            .arg(path)
            .status()
            .map_err(|e| e.to_string())?;
        if status.success() {
            Ok(())
        } else {
            Err("Finder 无法打开目录".into())
        }
    }
    #[cfg(not(target_os = "macos"))]
    app.opener()
        .open_path(path.to_string_lossy(), None::<&str>)
        .map_err(|e| e.to_string())
}

fn reveal_entry(app: &tauri::AppHandle, path: PathBuf) -> Result<(), String> {
    // Opener's reveal_item_in_dir canonicalizes the link on macOS/Linux.
    // Keep the lexical link path, and pass it as an argument without a shell.
    #[cfg(target_os = "macos")]
    {
        let _ = app;
        let status = std::process::Command::new("/usr/bin/open")
            .arg("-R")
            .arg(path)
            .status()
            .map_err(|e| e.to_string())?;
        if status.success() {
            Ok(())
        } else {
            Err("文件管理器无法定位软链目录".into())
        }
    }
    #[cfg(windows)]
    {
        let _ = app;
        std::process::Command::new("explorer.exe")
            .arg(format!("/select,{}", path.display()))
            .spawn()
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        let parent = path
            .parent()
            .ok_or_else(|| "软链目录没有上级文件夹".to_string())?;
        app.opener()
            .open_path(parent.to_string_lossy(), None::<&str>)
            .map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn directory_only_and_invalid_locations_are_rejected() {
        let temp = tempfile::tempdir().unwrap();
        let actual = fs::canonicalize(temp.path()).unwrap();
        assert_eq!(
            resolve(actual.to_str().unwrap(), false).unwrap(),
            DirectoryAction::Open(actual.clone())
        );
        let file = actual.join("script.sh");
        fs::write(&file, "echo must-not-run").unwrap();
        for original in [false, true] {
            assert!(resolve(file.to_str().unwrap(), original).is_err());
            assert!(resolve(actual.join("missing").to_str().unwrap(), original).is_err());
            assert!(resolve("https://example.com", original).is_err());
            assert!(resolve("relative/directory", original).is_err());
        }
    }
    #[test]
    #[cfg(target_os = "macos")]
    fn application_directories_are_revealed_without_launching() {
        let temp = tempfile::tempdir().unwrap();
        let path = fs::canonicalize(temp.path()).unwrap().join("Example.app");
        fs::create_dir(&path).unwrap();
        for original in [false, true] {
            assert_eq!(
                resolve(path.to_str().unwrap(), original).unwrap(),
                DirectoryAction::RevealEntry(path.clone())
            );
        }
    }
    #[test]
    #[cfg(unix)]
    fn original_link_location_differs_from_the_entity_even_for_nested_skills() {
        use std::os::unix::fs::symlink;
        let temp = tempfile::tempdir().unwrap();
        let base = fs::canonicalize(temp.path()).unwrap();
        let actual = base.join("entity");
        fs::create_dir_all(actual.join("child")).unwrap();
        fs::write(actual.join("SKILL.md"), "# Parent Skill").unwrap();
        let link = base.join("original");
        symlink(&actual, &link).unwrap();
        assert_eq!(
            resolve(link.to_str().unwrap(), false).unwrap(),
            DirectoryAction::Open(actual.clone())
        );
        assert_eq!(
            resolve(link.to_str().unwrap(), true).unwrap(),
            DirectoryAction::RevealEntry(link.clone())
        );
        assert_eq!(
            resolve(link.join("child").to_str().unwrap(), false).unwrap(),
            DirectoryAction::Open(actual.join("child"))
        );
        assert_eq!(
            resolve(link.join("child").to_str().unwrap(), true).unwrap(),
            DirectoryAction::RevealEntry(link.clone())
        );
        // An ordinary alias (such as macOS /tmp) is not an adopted parent Skill.
        fs::remove_file(actual.join("SKILL.md")).unwrap();
        assert_eq!(
            resolve(link.join("child").to_str().unwrap(), true).unwrap(),
            DirectoryAction::Open(actual.join("child"))
        );
        let broken = base.join("broken");
        symlink(base.join("gone"), &broken).unwrap();
        assert!(resolve(broken.to_str().unwrap(), false).is_err());
        assert_eq!(
            resolve(broken.to_str().unwrap(), true).unwrap(),
            DirectoryAction::RevealEntry(broken)
        );
    }
}
