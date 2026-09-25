use crate::error::{Result, fail};
use crate::model::{ScanItem, ScanResult};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    path::{Component, Path, PathBuf},
};
use walkdir::WalkDir;

const MAX_BYTES: u64 = 256 * 1024 * 1024;
pub fn absolute(path: &str) -> Result<PathBuf> {
    let expanded = if path == "~" || path.starts_with("~/") {
        let home = home()?;
        home.join(path.strip_prefix("~/").unwrap_or(""))
    } else {
        PathBuf::from(path)
    };
    if !expanded.is_absolute() {
        return fail("请输入绝对路径");
    }
    if expanded
        .components()
        .any(|c| matches!(c, Component::ParentDir))
    {
        return fail("路径不能包含 ..");
    }
    Ok(expanded)
}
pub fn home() -> Result<PathBuf> {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .map(PathBuf::from)
        .ok_or_else(|| crate::error::Error::Message("无法确定用户目录".into()))
}
pub fn default_config() -> Result<PathBuf> {
    if cfg!(windows) {
        Ok(PathBuf::from(
            std::env::var_os("LOCALAPPDATA")
                .ok_or_else(|| crate::error::Error::Message("LOCALAPPDATA 不可用".into()))?,
        )
        .join("SkillDock/config"))
    } else if cfg!(target_os = "linux") {
        Ok(std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or(home()?.join(".config"))
            .join("skilldock"))
    } else {
        Ok(home()?.join(".config/skilldock"))
    }
}
pub fn default_root() -> Result<PathBuf> {
    if cfg!(windows) {
        Ok(default_config()?.parent().unwrap().join("data"))
    } else if cfg!(target_os = "linux") {
        Ok(std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .unwrap_or(home()?.join(".local/share"))
            .join("skilldock"))
    } else {
        Ok(home()?.join(".local/share/skilldock"))
    }
}
pub fn exists(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok()
}
pub fn protected(path: &Path) -> bool {
    path.components()
        .any(|c| matches!(c.as_os_str().to_str(), Some(".system" | "plugins" | ".git")))
}
pub fn safe_relative(path: &str) -> Result<PathBuf> {
    let p = PathBuf::from(path);
    if p.is_absolute()
        || p.components().any(|c| {
            matches!(
                c,
                Component::ParentDir | Component::Prefix(_) | Component::RootDir
            )
        })
    {
        return fail("包内路径越界");
    }
    Ok(p)
}
pub fn clean_name(name: &str) -> Result<String> {
    let n = name.trim();
    if n.is_empty()
        || n.len() > 128
        || n.starts_with('.')
        || n.chars()
            .any(|c| c.is_control() || "<>:\"/\\|?*".contains(c))
        || n.ends_with([' ', '.'])
    {
        return fail("Skill 名称不能包含路径分隔符或系统保留字符");
    }
    let stem = n.split('.').next().unwrap_or(n).to_ascii_uppercase();
    if matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (stem.len() == 4
            && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.as_bytes()[3].is_ascii_digit())
    {
        return fail("名称是系统保留名");
    }
    Ok(n.into())
}
// Check directory entries, not a case-insensitive filesystem lookup: a reference
// named `skill.md` is not the `SKILL.md` entry point on macOS or Windows.
pub fn has_skill_entry(path: &Path) -> bool {
    fs::read_dir(path).is_ok_and(|entries| {
        entries.filter_map(|entry| entry.ok()).any(|entry| {
            entry.file_name() == std::ffi::OsStr::new("SKILL.md") && entry.path().is_file()
        })
    })
}

// Only reserve resource directories beneath an actual Skill. A package may
// legitimately have a top-level directory called references or scripts.
fn is_skill_resource(path: &Path) -> bool {
    path.ancestors().any(|directory| {
        matches!(
            directory.file_name().and_then(|name| name.to_str()),
            Some("references" | "assets" | "scripts")
        ) && directory.parent().is_some_and(has_skill_entry)
    })
}

pub fn metadata(path: &Path) -> Result<(String, String)> {
    let fallback = path.file_name().and_then(|s| s.to_str()).unwrap_or("skill");
    metadata_with_fallback(path, fallback)
}

pub(crate) fn metadata_with_fallback(path: &Path, fallback: &str) -> Result<(String, String)> {
    if !has_skill_entry(path) {
        return fail("目录缺少精确命名的 SKILL.md 入口，普通 skill.md 资料不作为成员");
    }

    let file = path.join("SKILL.md");
    if fs::metadata(&file)?.len() > 1024 * 1024 {
        return fail("SKILL.md 超过 1 MiB 限制");
    }
    let text = fs::read_to_string(file)?;
    let normalized = text.trim_start_matches('\u{feff}').replace("\r\n", "\n");
    let (name, description) = if let Some(rest) = normalized.strip_prefix("---\n") {
        let end = rest
            .find("\n---")
            .ok_or_else(|| crate::error::Error::Message("YAML 元信息没有结束标记".into()))?;
        let value: serde_yaml_ng::Value = serde_yaml_ng::from_str(&rest[..end])
            .map_err(|e| crate::error::Error::Message(format!("YAML 元信息错误：{e}")))?;
        (
            value
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or(fallback)
                .to_string(),
            value
                .get("description")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
        )
    } else {
        (
            fallback.into(),
            normalized
                .lines()
                .find(|l| !l.trim().is_empty() && !l.starts_with('#'))
                .unwrap_or("")
                .chars()
                .take(240)
                .collect(),
        )
    };
    Ok((clean_name(&name)?, description))
}
// Discovery exclusions must never be reused for copying or backup integrity.
fn included(entry: &walkdir::DirEntry) -> bool {
    let n = entry.file_name().to_string_lossy();
    !matches!(n.as_ref(), ".git" | "node_modules" | "target" | ".system")
        && !n.starts_with(".skilldock-")
}
pub fn scan(path: &Path) -> Result<ScanResult> {
    let mut result = ScanResult {
        root: path.display().to_string(),
        items: vec![],
        warnings: vec![],
    };
    if !path.is_dir() {
        return fail("扫描目录不存在或无权限");
    }
    for entry in WalkDir::new(path)
        .follow_links(false)
        .max_depth(12)
        .into_iter()
        .filter_entry(|entry| included(entry) && !is_skill_resource(entry.path()))
    {
        let e = match entry {
            Ok(e) => e,
            Err(e) => {
                result.warnings.push(e.to_string());
                continue;
            }
        };
        if e.file_type().is_symlink() {
            let target_path = fs::read_link(e.path()).ok();
            let is_alive = e.path().exists();
            let description = match &target_path {
                Some(target) => format!("指向：{}", target.display()),
                None => String::new(),
            };
            let (status, error) = if is_alive {
                let target_info = target_path
                    .as_ref()
                    .map(|t| format!("（指向 {}）", t.display()))
                    .unwrap_or_default();
                (
                    "linked",
                    format!(
                        "外部已有软链{}。SkillDock 完整保留原样，不自动接管；如需归集请选择原始实体目录。",
                        target_info
                    ),
                )
            } else {
                let target_info = target_path
                    .as_ref()
                    .map(|t| format!("（指向 {}）", t.display()))
                    .unwrap_or_default();
                (
                    "broken",
                    format!("软链目标不存在{}，链接已失效。", target_info),
                )
            };
            result.items.push(ScanItem {
                path: e.path().display().to_string(),
                name: e.file_name().to_string_lossy().into(),
                description,
                status: status.into(),
                error,
            });
            continue;
        }
        if e.file_type().is_dir() && has_skill_entry(e.path()) {
            let (name, description, status, error) = match metadata(e.path()) {
                Ok((n, d)) => (n, d, "ready".into(), String::new()),
                Err(error) => (
                    e.file_name().to_string_lossy().into(),
                    String::new(),
                    "invalid".into(),
                    error.to_string(),
                ),
            };
            result.items.push(ScanItem {
                path: e.path().display().to_string(),
                name,
                description,
                status,
                error,
            });
        }
    }
    Ok(result)
}
pub fn tree_digest(root: &Path) -> Result<String> {
    digest_tree(root, false, &[])
}

pub fn content_digest(root: &Path) -> Result<String> {
    digest_tree(root, true, &[])
}

pub fn snapshot_matches(root: &Path, expected: &str) -> Result<bool> {
    for include_lock in [false, true] {
        let generated = generated_python_paths(root, include_lock)?;
        if !generated.is_empty() && digest_tree(root, true, &generated)? == expected {
            return Ok(true);
        }
    }
    Ok(tree_digest(root)? == expected || content_digest(root)? == expected)
}

fn digest_tree(root: &Path, ignore_finder: bool, excluded: &[PathBuf]) -> Result<String> {
    let canonical = fs::canonicalize(root)?;
    if fs::symlink_metadata(root)?.file_type().is_symlink() {
        return fail("不能把软链自身作为新来源，请选择原始内容目录");
    }
    let mut hash = Sha256::new();
    let mut bytes = 0u64;
    let mut count = 0usize;
    for entry in WalkDir::new(root)
        .follow_links(false)
        .sort_by_file_name()
        .into_iter()
        .filter_entry(|e| !excluded.iter().any(|p| e.path().starts_with(p)))
    {
        let e = entry.map_err(|e| crate::error::Error::Message(e.to_string()))?;
        let rel = e.path().strip_prefix(root).unwrap();
        // Finder writes these view preferences while browsing an otherwise unchanged Skill.
        if ignore_finder && e.file_type().is_file() && e.file_name() == ".DS_Store" {
            continue;
        }
        if rel.as_os_str().is_empty() {
            continue;
        }
        count += 1;
        if count > 25000 {
            return fail("来源超过 25000 个文件/目录限制");
        }
        let text = rel
            .to_str()
            .ok_or_else(|| crate::error::Error::Message("来源包含非 UTF-8 文件名".into()))?
            .replace('\\', "/");
        hash.update((text.len() as u64).to_le_bytes());
        hash.update(text.as_bytes());
        if e.file_type().is_symlink() {
            let dest = fs::read_link(e.path())?;
            if dest.is_absolute() || !fs::canonicalize(e.path())?.starts_with(&canonical) {
                return fail(format!("资源软链逃逸包边界：{}", e.path().display()));
            }
            let raw = dest
                .to_str()
                .ok_or_else(|| crate::error::Error::Message("链接路径编码无效".into()))?;
            hash.update(b"L");
            hash.update((raw.len() as u64).to_le_bytes());
            hash.update(raw.as_bytes());
        } else if e.file_type().is_dir() {
            hash.update(b"D");
        } else if e.file_type().is_file() {
            let meta = fs::metadata(e.path())?;
            bytes = bytes
                .checked_add(meta.len())
                .ok_or_else(|| crate::error::Error::Message("来源过大".into()))?;
            if bytes > MAX_BYTES {
                return fail("来源内容超过 256 MiB 限制");
            }
            hash.update(b"F");
            hash.update(meta.len().to_le_bytes());
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                hash.update((meta.permissions().mode() & 0o111).to_le_bytes());
            }
            let mut f = fs::File::open(e.path())?;
            let mut buf = [0u8; 65536];
            loop {
                let n = f.read(&mut buf)?;
                if n == 0 {
                    break;
                }
                hash.update(&buf[..n]);
            }
        } else {
            return fail(format!("来源包含不支持的特殊文件：{}", e.path().display()));
        }
    }
    Ok(format!("{:x}", hash.finalize()))
}
/// Exclusions apply to portable local imports only, never to backups or adoption.
pub fn local_import_exclusions(root: &Path) -> Result<Vec<PathBuf>> {
    let mut excluded = vec![];
    let mut entries = WalkDir::new(root).follow_links(false).into_iter();
    while let Some(entry) = entries.next() {
        let entry = entry.map_err(|e| crate::error::Error::Message(e.to_string()))?;
        if entry.depth() == 0 {
            continue;
        }
        let name = entry.file_name().to_string_lossy();
        if name == ".git"
            || (matches!(name.as_ref(), ".venv" | "__pycache__")
                && (entry.file_type().is_dir() || entry.file_type().is_symlink()))
        {
            excluded.push(entry.path().to_path_buf());
            if entry.file_type().is_dir() {
                entries.skip_current_dir();
            }
        }
    }
    Ok(excluded)
}

pub fn copy_tree(from: &Path, to: &Path) -> Result<()> {
    copy_tree_excluding(from, to, &[])
}
pub fn copy_tree_excluding(from: &Path, to: &Path, excluded: &[PathBuf]) -> Result<()> {
    fs::create_dir_all(to)?;
    for entry in WalkDir::new(from)
        .follow_links(false)
        .sort_by_file_name()
        .into_iter()
        .filter_entry(|e| !excluded.iter().any(|p| e.path().starts_with(p)))
    {
        let e = entry.map_err(|e| crate::error::Error::Message(e.to_string()))?;
        let rel = e.path().strip_prefix(from).unwrap();
        if rel.as_os_str().is_empty() {
            continue;
        }
        let dst = to.join(rel);
        if e.file_type().is_dir() {
            fs::create_dir_all(&dst)?;
        } else if e.file_type().is_file() {
            if let Some(p) = dst.parent() {
                fs::create_dir_all(p)?;
            }
            fs::copy(e.path(), &dst)?;
        } else if e.file_type().is_symlink() {
            create_link(&fs::read_link(e.path())?, &dst, e.path().is_dir())?;
        } else {
            return fail(format!("不支持的文件类型：{}", e.path().display()));
        }
    }
    Ok(())
}
pub fn snapshot_tree(root: &Path, source: &Path) -> Result<String> {
    snapshot_tree_with(root, source, tree_digest, false)
}

pub fn snapshot_current_content(root: &Path, source: &Path) -> Result<String> {
    snapshot_tree_with(root, source, content_digest, true)
}

fn snapshot_tree_with(
    root: &Path,
    source: &Path,
    digest_fn: fn(&Path) -> Result<String>,
    allow_generated: bool,
) -> Result<String> {
    let digest = digest_fn(source)?;
    let target = root.join("objects").join(&digest).join("tree");
    if target.exists() {
        if !(if allow_generated {
            snapshot_matches(&target, &digest)?
        } else {
            digest_fn(&target)? == digest
        }) {
            return fail("中央库快照已被外部修改，请先修复");
        }
        return Ok(digest);
    }
    fs::create_dir_all(root.join("objects"))?;
    let temp = tempfile::Builder::new()
        .prefix(".skilldock-")
        .tempdir_in(root.join("objects"))?;
    copy_tree(source, &temp.path().join("tree"))?;
    if digest_fn(&temp.path().join("tree"))? != digest || digest_fn(source)? != digest {
        return fail("复制期间来源发生变化，请重新扫描");
    }
    fs::rename(temp.path(), target.parent().unwrap())?;
    Ok(digest)
}
pub fn atomic_json(path: &Path, value: &impl Serialize) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| crate::error::Error::Message("无效保存路径".into()))?;
    fs::create_dir_all(parent)?;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    serde_json::to_writer_pretty(file.as_file_mut(), value)?;
    file.write_all(b"\n")?;
    file.as_file().sync_all()?;
    file.persist(path).map_err(|e| e.error)?;
    #[cfg(unix)]
    fs::File::open(parent)?.sync_all()?;
    Ok(())
}
pub fn create_link(target: &Path, link: &Path, is_dir: bool) -> Result<()> {
    #[cfg(unix)]
    {
        let _ = is_dir;
        std::os::unix::fs::symlink(target, link)?;
    }
    #[cfg(windows)]
    {
        if is_dir {
            std::os::windows::fs::symlink_dir(target, link)?;
        } else {
            std::os::windows::fs::symlink_file(target, link)?;
        }
    }
    Ok(())
}
pub fn remove_link(path: &Path) -> Result<()> {
    if !fs::symlink_metadata(path)?.file_type().is_symlink() {
        return fail("目标已不是软链，拒绝删除");
    }
    #[cfg(unix)]
    fs::remove_file(path)?;
    #[cfg(windows)]
    {
        use std::os::windows::fs::FileTypeExt;
        if fs::symlink_metadata(path)?.file_type().is_symlink_dir() {
            fs::remove_dir(path)?;
        } else {
            fs::remove_file(path)?;
        }
    }
    Ok(())
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Change {
    pub path: PathBuf,
    pub before: Option<PathBuf>,
    pub after: Option<PathBuf>,
    pub backup: Option<PathBuf>,
    #[serde(default)]
    pub restore: bool,
    #[serde(default)]
    pub backup_digest: Option<String>,
}
pub fn apply(change: &Change) -> Result<()> {
    if change.restore {
        let backup = change
            .backup
            .as_ref()
            .ok_or_else(|| crate::error::Error::Message("恢复记录缺少备份".into()))?;
        if let Some(parent) = change.path.parent() {
            if fs::canonicalize(parent)? != parent {
                return fail("原目录父路径已变化");
            }
        }
        if let Some(expected) = &change.backup_digest {
            if manifest_digest(backup)? != *expected {
                return fail("备份在校验后发生变化");
            }
        } else if !backup.is_dir() {
            return fail("备份缺失");
        }
        if let Some(link) = &change.after {
            if fs::read_link(&change.path).ok().as_ref() != Some(link) {
                return fail("原位置链接已变化");
            }
            remove_link(&change.path)?;
        } else if exists(&change.path) {
            return fail("原位置已被其他内容占用");
        }
        fs::rename(backup, &change.path)?;
        return Ok(());
    }
    if let Some(parent) = change.path.parent()
        && fs::canonicalize(parent)? != parent
    {
        return fail("目标父目录发生变化，拒绝写入");
    }
    if let Some(backup) = &change.backup {
        if !change.path.is_dir() || fs::symlink_metadata(&change.path)?.file_type().is_symlink() {
            return fail("归集来源已变化，请重新扫描");
        }
        if exists(backup) {
            return fail("备份路径已存在");
        }
        if let Some(expected) = &change.backup_digest {
            if manifest_digest(&change.path)? != *expected {
                return fail("归集来源在校验后发生变化，未替换");
            }
        }
        fs::rename(&change.path, backup)?;
    } else if let Some(before) = &change.before {
        if fs::read_link(&change.path).ok().as_ref() != Some(before) {
            return fail("链接被外部修改，拒绝覆盖");
        }
        remove_link(&change.path)?;
    } else if exists(&change.path) {
        return fail(format!("目标已存在：{}", change.path.display()));
    }
    if let Some(after) = &change.after {
        create_link(after, &change.path, true)?;
    }
    Ok(())
}
pub fn undo(change: &Change) -> Result<()> {
    if change.restore {
        let backup = change
            .backup
            .as_ref()
            .ok_or_else(|| crate::error::Error::Message("恢复记录缺少备份路径".into()))?;
        if !exists(backup) {
            if let Some(expected) = &change.backup_digest {
                if manifest_digest(&change.path)? != *expected {
                    return fail("恢复后的目录被修改，停止回滚");
                }
            }
            fs::rename(&change.path, backup)?;
        }
        if let Some(link) = &change.after {
            if !exists(&change.path) {
                create_link(link, &change.path, true)?;
            } else if fs::read_link(&change.path).ok().as_ref() != Some(link) {
                return fail("恢复路径被外部内容占用");
            }
        } else if exists(&change.path) {
            return fail("恢复路径被外部内容占用");
        }
        return Ok(());
    }
    if let Some(backup) = &change.backup {
        if !exists(backup) {
            return Ok(());
        }
        if exists(&change.path) {
            if fs::read_link(&change.path).ok().as_ref() == change.after.as_ref() {
                remove_link(&change.path)?;
            } else {
                return fail("恢复路径被外部文件占用，备份已保留");
            }
        }
        fs::rename(backup, &change.path)?;
        return Ok(());
    }
    if let Some(before) = &change.before
        && fs::read_link(&change.path).ok().as_ref() == Some(before)
    {
        return Ok(());
    }
    if exists(&change.path) {
        if change.after.is_some()
            && fs::read_link(&change.path).ok().as_ref() == change.after.as_ref()
        {
            remove_link(&change.path)?;
        } else {
            return fail("恢复时检测到外部内容，未覆盖");
        }
    }
    if let Some(before) = &change.before {
        create_link(before, &change.path, true)?;
    }
    Ok(())
}

/// Deduplicate physical scan roots first, then Skill directories shared by
/// overlapping roots. Do not merge distinct Skill folders just for equal names.
pub fn scan_many(paths: &[String]) -> Result<ScanResult> {
    let mut roots = std::collections::BTreeMap::<PathBuf, PathBuf>::new();
    for path in paths {
        let path = absolute(path)?;
        let physical = fs::canonicalize(&path)?;
        match roots.get(&physical) {
            Some(existing) if !fs::symlink_metadata(existing)?.file_type().is_symlink() => {}
            _ => {
                roots.insert(physical, path);
            }
        }
    }
    let mut result = ScanResult {
        root: String::new(),
        items: vec![],
        warnings: vec![],
    };
    let mut seen = std::collections::BTreeSet::new();
    for path in roots.values() {
        let scanned = scan(path)?;
        result.warnings.extend(scanned.warnings);
        for item in scanned.items {
            let path = PathBuf::from(&item.path);
            // Existing links are separate installation locations, not skill
            // content. Keep each link visible without traversing its target.
            let key = if item.status == "linked" || item.status == "broken" {
                path.parent()
                    .and_then(|parent| fs::canonicalize(parent).ok())
                    .map(|parent| parent.join(path.file_name().unwrap_or_default()))
                    .unwrap_or(path)
            } else {
                fs::canonicalize(&path).unwrap_or(path)
            };
            if seen.insert(key) {
                result.items.push(item);
            }
        }
    }
    Ok(result)
}

#[cfg(test)]
mod scan_dedup_tests {
    use super::*;
    #[test]
    fn duplicate_and_overlapping_roots_return_each_skill_once() {
        let dir = tempfile::tempdir().unwrap();
        let skill = dir.path().join("nested/example");
        fs::create_dir_all(&skill).unwrap();
        fs::write(skill.join("SKILL.md"), "# Example").unwrap();
        let root = dir.path().display().to_string();
        let scan = scan_many(&[
            root.clone(),
            format!("{root}/"),
            skill.display().to_string(),
        ])
        .unwrap();
        assert_eq!(
            scan.items
                .iter()
                .filter(|item| item.status == "ready")
                .count(),
            1
        );
    }
    #[cfg(unix)]
    #[test]
    fn alias_root_and_real_root_are_scanned_once() {
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().join("real");
        let alias = dir.path().join("alias");
        fs::create_dir_all(real.join("example")).unwrap();
        fs::write(real.join("example/SKILL.md"), "# Example").unwrap();
        std::os::unix::fs::symlink(&real, &alias).unwrap();
        let result = scan_many(&[alias.display().to_string(), real.display().to_string()]).unwrap();
        assert_eq!(result.items.len(), 1);
        assert_eq!(result.items[0].status, "ready");
    }
    #[test]
    fn same_name_in_different_folders_is_not_merged() {
        let dir = tempfile::tempdir().unwrap();
        for name in ["a", "b"] {
            fs::create_dir_all(dir.path().join(name)).unwrap();
            fs::write(
                dir.path().join(name).join("SKILL.md"),
                "---\nname: example\n---\n",
            )
            .unwrap();
        }
        assert_eq!(
            scan_many(&[dir.path().display().to_string()])
                .unwrap()
                .items
                .len(),
            2
        );
    }
}

/// A portable inventory. Symlinks are hashed as link text, never followed.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TreeEntry {
    pub relative: String,
    pub kind: String,
    pub digest: String,
}

pub fn tree_manifest(root: &Path, legacy_filter: bool) -> Result<Vec<TreeEntry>> {
    if !fs::symlink_metadata(root)?.is_dir() {
        return fail("校验目录不是实体文件夹");
    }
    let mut entries = vec![];
    for entry in WalkDir::new(root)
        .follow_links(false)
        .sort_by_file_name()
        .into_iter()
        .filter_entry(|e| e.depth() == 0 || !legacy_filter || included(e))
    {
        let entry = entry.map_err(|e| crate::error::Error::Message(e.to_string()))?;
        if entry.depth() == 0 {
            continue;
        }
        let relative = entry
            .path()
            .strip_prefix(root)
            .unwrap()
            .to_str()
            .ok_or_else(|| crate::error::Error::Message("文件名编码无效".into()))?
            .replace('\\', "/");
        entries.push(manifest_entry(entry.path(), relative)?);
    }
    Ok(entries)
}
pub fn manifest_entry(path: &Path, relative: String) -> Result<TreeEntry> {
    let file_type = fs::symlink_metadata(path)?.file_type();
    let mut hash = Sha256::new();
    let kind = if file_type.is_symlink() {
        let link = fs::read_link(path)?;
        hash.update(
            link.to_str()
                .ok_or_else(|| crate::error::Error::Message("链接编码无效".into()))?
                .as_bytes(),
        );
        "link"
    } else if file_type.is_dir() {
        "directory"
    } else if file_type.is_file() {
        let meta = fs::metadata(path)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            hash.update((meta.permissions().mode() & 0o111).to_le_bytes());
        }
        let mut input = fs::File::open(path)?;
        let mut buffer = [0u8; 65536];
        loop {
            let count = input.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            hash.update(&buffer[..count]);
        }
        "file"
    } else {
        return fail(format!("来源包含不支持的特殊文件：{}", path.display()));
    };
    Ok(TreeEntry {
        relative,
        kind: kind.into(),
        digest: format!("{:x}", hash.finalize()),
    })
}
pub fn manifest_digest(root: &Path) -> Result<String> {
    Ok(format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&tree_manifest(root, false)?)?)
    ))
}
// Recognize the bootstrap contract shipped by these Skills; a manifest alone is insufficient.
pub fn rebuildable_python_skill(root: &Path) -> bool {
    let regular = |p: &Path| fs::symlink_metadata(p).is_ok_and(|m| m.is_file());
    if !regular(&root.join("SKILL.md"))
        || !regular(&root.join("requirements.txt"))
        || !regular(&root.join("scripts/bootstrap.py"))
    {
        return false;
    }
    let Ok(requirements) = fs::read_to_string(root.join("requirements.txt")) else {
        return false;
    };
    if !requirements
        .lines()
        .any(|line| !line.trim().is_empty() && !line.trim().starts_with('#'))
    {
        return false;
    }
    let Ok(bootstrap) = fs::read_to_string(root.join("scripts/bootstrap.py")) else {
        return false;
    };
    if ![
        "def ensure_skill_deps(",
        "venv.EnvBuilder(",
        "_parse_requirements(",
        "_install(",
    ]
    .iter()
    .all(|part| bootstrap.contains(part))
    {
        return false;
    }
    fs::read_dir(root.join("scripts"))
        .ok()
        .is_some_and(|entries| {
            entries.flatten().any(|entry| {
                entry.file_name() != "bootstrap.py"
                    && entry.path().extension().is_some_and(|e| e == "py")
                    && regular(&entry.path())
                    && fs::read_to_string(entry.path()).is_ok_and(|text| {
                        text.contains("from bootstrap import ensure_skill_deps")
                            && text
                                .lines()
                                .any(|line| line.trim() == "ensure_skill_deps(__file__)")
                    })
            })
        })
}

pub fn python_environment_issues(root: &Path) -> Result<Vec<String>> {
    Ok(local_import_exclusions(root)?.into_iter()
        .filter(|p| p.file_name().is_some_and(|name| name == ".venv"))
        .filter(|p| !p.parent().is_some_and(rebuildable_python_skill))
        .map(|p| format!("运行环境 {} 未复制，未找到可识别的依赖清单及启动重建入口；请配置可迁移依赖后重新同步。", p.display())).collect())
}

fn generated_python_paths(root: &Path, include_lock: bool) -> Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    let mut walker = WalkDir::new(root).follow_links(false).into_iter();
    while let Some(entry) = walker.next() {
        let entry = entry.map_err(|e| crate::error::Error::Message(e.to_string()))?;
        if entry.file_type().is_dir()
            && [".venv", "__pycache__", ".git"]
                .iter()
                .any(|n| entry.file_name() == *n)
        {
            walker.skip_current_dir();
            continue;
        }
        if entry.file_name() != "SKILL.md" || !entry.file_type().is_file() {
            continue;
        }
        let skill = entry.path().parent().unwrap();
        if !rebuildable_python_skill(skill) {
            continue;
        }
        if fs::symlink_metadata(skill.join(".venv")).is_ok_and(|m| m.is_dir()) {
            paths.push(skill.join(".venv"));
        }
        if include_lock
            && fs::symlink_metadata(skill.join(".venv.lock"))
                .is_ok_and(|m| m.is_file() && m.len() == 0)
        {
            paths.push(skill.join(".venv.lock"));
        }
        for sub in WalkDir::new(skill)
            .follow_links(false)
            .into_iter()
            .filter_entry(|e| e.file_name() != ".venv")
        {
            let sub = sub.map_err(|e| crate::error::Error::Message(e.to_string()))?;
            if sub.file_name() == "__pycache__" && sub.file_type().is_dir() {
                paths.push(sub.path().to_path_buf());
            }
        }
    }
    Ok(paths)
}

#[cfg(test)]
mod python_runtime_tests {
    use super::*;
    fn fixture(root: &Path) {
        fs::create_dir_all(root.join("scripts")).unwrap();
        fs::write(root.join("SKILL.md"), "---\nname: demo\n---\nSkill").unwrap();
        fs::write(root.join("requirements.txt"), "requests\n").unwrap();
        fs::write(root.join("scripts/bootstrap.py"), "def ensure_skill_deps(caller):\n    venv.EnvBuilder()\n    _parse_requirements()\n    _install()\n").unwrap();
        fs::write(
            root.join("scripts/run.py"),
            "from bootstrap import ensure_skill_deps\nensure_skill_deps(__file__)\n",
        )
        .unwrap();
    }
    #[test]
    fn runtime_additions_do_not_hide_source_edits_or_change_backup_digest_rules() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        fixture(root);
        let digest = content_digest(root).unwrap();
        fs::create_dir_all(root.join(".venv/bin")).unwrap();
        fs::write(root.join(".venv/bin/generated"), "runtime").unwrap();
        fs::write(root.join(".venv.lock"), "").unwrap();
        fs::create_dir_all(root.join("scripts/__pycache__")).unwrap();
        fs::write(root.join("scripts/__pycache__/run.pyc"), "cache").unwrap();
        assert!(snapshot_matches(root, &digest).unwrap());
        assert_ne!(tree_digest(root).unwrap(), digest);
        assert!(python_environment_issues(root).unwrap().is_empty());
        fs::write(root.join("scripts/run.py"), "from bootstrap import ensure_skill_deps\nensure_skill_deps(__file__)\nprint('changed')").unwrap();
        assert!(!snapshot_matches(root, &digest).unwrap());
    }
    #[test]
    fn manifest_alone_does_not_remove_environment_blocker() {
        let temp = tempfile::tempdir().unwrap();
        fixture(temp.path());
        fs::create_dir(temp.path().join(".venv")).unwrap();
        fs::remove_file(temp.path().join("scripts/run.py")).unwrap();
        assert_eq!(python_environment_issues(temp.path()).unwrap().len(), 1);
    }
    #[test]
    fn stored_empty_lock_remains_compatible_and_tracked_environment_stays_strict() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        fixture(root);
        fs::write(root.join(".venv.lock"), "").unwrap();
        let old = content_digest(root).unwrap();
        fs::create_dir(root.join(".venv")).unwrap();
        fs::write(root.join(".venv/data"), "a").unwrap();
        assert!(snapshot_matches(root, &old).unwrap());
        let full = tree_digest(root).unwrap();
        fs::write(root.join(".venv/data"), "b").unwrap();
        assert!(!snapshot_matches(root, &full).unwrap());
    }
}
