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
pub fn metadata(path: &Path) -> Result<(String, String)> {
    let file = path.join("SKILL.md");
    if fs::metadata(&file)?.len() > 1024 * 1024 {
        return fail("SKILL.md 超过 1 MiB 限制");
    }
    let text = fs::read_to_string(file)?;
    let fallback = path.file_name().and_then(|s| s.to_str()).unwrap_or("skill");
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
        .filter_entry(included)
    {
        let e = match entry {
            Ok(e) => e,
            Err(e) => {
                result.warnings.push(e.to_string());
                continue;
            }
        };
        if e.file_type().is_symlink() {
            result.items.push(ScanItem {
                path: e.path().display().to_string(),
                name: e.file_name().to_string_lossy().into(),
                description: String::new(),
                status: if e.path().exists() {
                    "linked"
                } else {
                    "broken"
                }
                .into(),
                error: "现有软链仅展示，不自动接管或跟随扫描".into(),
            });
            continue;
        }
        if e.file_type().is_dir() && e.path().join("SKILL.md").is_file() {
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
        .filter_entry(included)
    {
        let e = entry.map_err(|e| crate::error::Error::Message(e.to_string()))?;
        let rel = e.path().strip_prefix(root).unwrap();
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
            return fail("来源包含不支持的特殊文件");
        }
    }
    Ok(format!("{:x}", hash.finalize()))
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
        .filter_entry(|e| included(e) && !excluded.iter().any(|p| e.path().starts_with(p)))
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
            return fail("不支持的文件类型");
        }
    }
    Ok(())
}
pub fn snapshot_tree(root: &Path, source: &Path) -> Result<String> {
    let digest = tree_digest(source)?;
    let target = root.join("objects").join(&digest).join("tree");
    if target.exists() {
        if tree_digest(&target)? != digest {
            return fail("中央库快照已被外部修改，请先修复");
        }
        return Ok(digest);
    }
    fs::create_dir_all(root.join("objects"))?;
    let temp = tempfile::Builder::new()
        .prefix(".skilldock-")
        .tempdir_in(root.join("objects"))?;
    copy_tree(source, &temp.path().join("tree"))?;
    if tree_digest(&temp.path().join("tree"))? != digest || tree_digest(source)? != digest {
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
}
pub fn apply(change: &Change) -> Result<()> {
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
