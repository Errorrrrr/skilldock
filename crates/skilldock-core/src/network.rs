use crate::error::{Error, Result, fail};
use crate::model::{CatalogItem, CatalogResult};
use reqwest::{Client, Response, Url, header};
use serde::Deserialize;
use std::collections::HashSet;
use std::ffi::OsStr;
use std::fs::{self, OpenOptions};
use std::io::{Cursor, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::process::{Output, Stdio};
use std::sync::Arc;
use std::time::Duration;
use tokio::process::Command;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;
use uuid::Uuid;
use zip::ZipArchive;

const CLAWHUB_SITE: &str = "https://clawhub.ai";
const USER_AGENT: &str = concat!("SkillDock/", env!("CARGO_PKG_VERSION"));
const SEARCH_LIMIT: usize = 50;
const MAX_SEARCH_RESULTS: usize = 200;
const MAX_SEARCH_SITES: usize = 8;
const SEARCH_CONCURRENCY: usize = 4;
const MAX_QUERY_CHARS: usize = 256;
const MAX_SLUG_CHARS: usize = 128;
const MAX_JSON_BYTES: usize = 4 * 1024 * 1024;
const MAX_ERROR_BYTES: usize = 16 * 1024;
const MAX_DOWNLOAD_BYTES: usize = 100 * 1024 * 1024;
const MAX_EXTRACTED_BYTES: u64 = 100 * 1024 * 1024;
const MAX_ARCHIVE_ENTRIES: usize = 10_000;
const MAX_ARCHIVE_PATH_BYTES: usize = 4_096;
const SEARCH_TIMEOUT: Duration = Duration::from_secs(30);
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(180);
const GIT_CLONE_TIMEOUT: Duration = Duration::from_secs(300);
const GIT_COMMAND_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Debug, Clone)]
pub struct PreparedSource {
    pub scan_subdir: String,
    pub path: PathBuf,
    pub kind: String,
    pub name: String,
    pub url: String,
    pub reference: String,
    pub version: String,
}

#[derive(Clone)]
struct SiteEndpoint {
    canonical: String,
    api: Url,
}

fn nullable_string<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<String, D::Error> {
    Ok(Option::<String>::deserialize(deserializer)?.unwrap_or_default())
}

#[derive(Debug, Deserialize)]
struct SearchEnvelope {
    #[serde(default)]
    results: Vec<SearchEntry>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SearchEntry {
    #[serde(default, deserialize_with = "nullable_string")]
    owner_handle: String,
    #[serde(default)]
    slug: String,
    #[serde(default, alias = "name", deserialize_with = "nullable_string")]
    display_name: String,
    #[serde(default, alias = "description", deserialize_with = "nullable_string")]
    summary: String,
    #[serde(default, deserialize_with = "nullable_string")]
    version: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SkillDetailEnvelope {
    skill: SkillDetail,
    #[serde(default)]
    latest_version: Option<VersionDetail>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SkillDetail {
    #[serde(default)]
    slug: String,
    #[serde(default, alias = "name", deserialize_with = "nullable_string")]
    display_name: String,
    #[serde(default)]
    tags: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct VersionDetail {
    #[serde(default, deserialize_with = "nullable_string")]
    version: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GithubHandoff {
    source_ref: String,
    repo: String,
    commit: String,
    #[serde(default)]
    path: String,
    #[serde(default)]
    content_hash: String,
    #[serde(default)]
    archive_url: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum GitTransport {
    Https,
    Ssh,
    Local,
}

#[derive(Debug)]
struct ValidatedGitSource {
    url: String,
    transport: GitTransport,
}

pub async fn search(query: &str, sites: &[String]) -> Result<CatalogResult> {
    let query = query.trim();
    if query.is_empty() {
        return fail("搜索关键词不能为空");
    }
    if query.chars().count() > MAX_QUERY_CHARS {
        return fail(format!("搜索关键词不能超过 {MAX_QUERY_CHARS} 个字符"));
    }

    let requested_sites = if sites.is_empty() {
        vec!["clawhub".to_owned()]
    } else {
        sites.to_vec()
    };

    let mut endpoints = Vec::new();
    let mut errors = Vec::new();
    let mut seen = HashSet::new();
    for (index, site) in requested_sites.into_iter().enumerate() {
        if index >= MAX_SEARCH_SITES {
            errors.push(format!(
                "站点数量超过上限 {MAX_SEARCH_SITES}，已跳过：{}",
                site.trim()
            ));
            continue;
        }
        match normalize_site(&site) {
            Ok(endpoint) => {
                if seen.insert(endpoint.canonical.clone()) {
                    endpoints.push(endpoint);
                }
            }
            Err(error) => errors.push(format!("{}：{error}", display_site(&site))),
        }
    }

    let client = http_client(SEARCH_TIMEOUT).await?;
    let semaphore = Arc::new(Semaphore::new(SEARCH_CONCURRENCY));
    let mut tasks = JoinSet::new();
    for (index, endpoint) in endpoints.into_iter().enumerate() {
        let client = client.clone();
        let semaphore = semaphore.clone();
        let query = query.to_owned();
        tasks.spawn(async move {
            let permit = semaphore
                .acquire_owned()
                .await
                .map_err(|_| Error::Message("搜索并发控制器已关闭".to_owned()))?;
            let result = search_site(&client, &query, &endpoint).await;
            drop(permit);
            Ok::<_, Error>((index, endpoint.canonical, result))
        });
    }

    let mut site_results = Vec::new();
    while let Some(joined) = tasks.join_next().await {
        match joined {
            Ok(Ok(result)) => site_results.push(result),
            Ok(Err(error)) => errors.push(format!("搜索任务失败：{error}")),
            Err(error) => errors.push(format!("搜索任务异常结束：{error}")),
        }
    }
    site_results.sort_by_key(|(index, _, _)| *index);

    let mut items = Vec::new();
    for (_, site, result) in site_results {
        match result {
            Ok(mut site_items) => items.append(&mut site_items),
            Err(error) => errors.push(format!("{site}：{error}")),
        }
    }
    if items.len() > MAX_SEARCH_RESULTS {
        items.truncate(MAX_SEARCH_RESULTS);
        errors.push(format!(
            "聚合搜索结果超过上限 {MAX_SEARCH_RESULTS}，其余结果已省略"
        ));
    }

    Ok(CatalogResult { items, errors })
}

pub async fn prepare_git(
    cache: &Path,
    url: &str,
    reference: &str,
    subdir: &str,
) -> Result<PreparedSource> {
    let source = validate_git_source(url)?;
    let reference = validate_git_reference(reference)?;
    let subdir = validate_relative_path(subdir, "Git 子目录")?;
    tokio::fs::create_dir_all(cache).await.map_err(|error| {
        Error::Message(format!("无法创建来源缓存目录 {}：{error}", cache.display()))
    })?;

    let checkout = unique_cache_path(cache, "git");
    let result = prepare_git_inner(&source, &reference, &subdir, &checkout).await;
    if result.is_err() {
        let _ = tokio::fs::remove_dir_all(&checkout).await;
    }
    result
}

pub async fn prepare_catalog(cache: &Path, slug: &str, site: &str) -> Result<PreparedSource> {
    let endpoint = normalize_site(site)?;
    if endpoint.canonical == "https://skills.sh" {
        return prepare_skills_sh(cache, slug).await;
    }
    let requested = slug.trim().to_string();
    let (owner, raw_slug) =
        if let Some((owner, raw)) = requested.trim_start_matches('@').split_once('/') {
            (Some(validate_slug(owner)?), raw)
        } else {
            (None, requested.as_str())
        };
    let slug = validate_slug(raw_slug)?;
    let metadata_client = http_client(SEARCH_TIMEOUT).await?;

    let mut detail_url = endpoint_url(&endpoint.api, &["skills", &slug])?;
    detail_url.set_query(None);
    if let Some(owner) = &owner {
        detail_url
            .query_pairs_mut()
            .append_pair("ownerHandle", owner);
    }
    let detail_response = metadata_client
        .get(detail_url.clone())
        .send()
        .await
        .map_err(|error| http_error("读取目录详情", &detail_url, error))?;
    ensure_https_response(&detail_response, "目录详情")?;
    let detail_response = ensure_success("读取目录详情", detail_response).await?;
    let detail_bytes = read_response_limited(detail_response, MAX_JSON_BYTES, "目录详情").await?;
    let detail: SkillDetailEnvelope = serde_json::from_slice(&detail_bytes).map_err(|error| {
        Error::Message(format!(
            "目录站点 {} 返回了无效的技能详情 JSON：{error}",
            endpoint.canonical
        ))
    })?;

    if !detail.skill.slug.is_empty() && detail.skill.slug != slug {
        return fail(format!(
            "目录详情标识不一致：请求 {slug}，响应 {}",
            detail.skill.slug
        ));
    }
    let version = detail
        .latest_version
        .as_ref()
        .map(|item| item.version.trim())
        .filter(|value| !value.is_empty())
        .or_else(|| {
            detail
                .skill
                .tags
                .get("latest")
                .and_then(|value| value.as_str())
        })
        .ok_or_else(|| Error::Message(format!("目录技能 {slug} 没有可下载版本")))?
        .to_owned();
    validate_catalog_version(&version)?;
    let name = if detail.skill.display_name.trim().is_empty() {
        slug.clone()
    } else {
        detail.skill.display_name.trim().to_owned()
    };

    let mut download_url = endpoint_url(&endpoint.api, &["download"])?;
    download_url
        .query_pairs_mut()
        .append_pair("slug", &slug)
        .append_pair("version", &version);
    if let Some(owner) = &owner {
        download_url
            .query_pairs_mut()
            .append_pair("ownerHandle", owner);
    }
    let download_client = http_client(DOWNLOAD_TIMEOUT).await?;
    let download_response = download_client
        .get(download_url.clone())
        .send()
        .await
        .map_err(|error| http_error("下载目录技能", &download_url, error))?;
    ensure_https_response(&download_response, "目录下载")?;
    let download_response = ensure_success("下载目录技能", download_response).await?;

    let is_json = download_response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.to_ascii_lowercase().contains("application/json"));

    if is_json {
        let body = read_response_limited(download_response, MAX_JSON_BYTES, "目录下载描述").await?;
        let handoff: GithubHandoff = serde_json::from_slice(&body).map_err(|error| {
            Error::Message(format!(
                "目录技能 {slug} 返回 JSON 而不是 ZIP，但响应不是受支持的 public-github 描述：{error}"
            ))
        })?;
        return prepare_github_handoff(cache, &requested, &name, &endpoint.canonical, &handoff)
            .await;
    }

    let content_type = download_response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("未提供")
        .to_owned();
    let archive =
        read_response_limited(download_response, MAX_DOWNLOAD_BYTES, "目录技能下载").await?;
    if archive
        .iter()
        .copied()
        .find(|byte| !byte.is_ascii_whitespace())
        == Some(b'{')
    {
        let handoff: GithubHandoff = serde_json::from_slice(&archive).map_err(|error| {
            Error::Message(format!(
                "目录技能 {slug} 返回 JSON 而不是 ZIP，但响应不是受支持的 public-github 描述：{error}"
            ))
        })?;
        return prepare_github_handoff(cache, &requested, &name, &endpoint.canonical, &handoff)
            .await;
    }
    tokio::fs::create_dir_all(cache).await.map_err(|error| {
        Error::Message(format!("无法创建来源缓存目录 {}：{error}", cache.display()))
    })?;
    let destination = unique_cache_path(cache, "catalog");
    let extraction_destination = destination.clone();
    let extraction =
        match tokio::task::spawn_blocking(move || extract_zip(&archive, &extraction_destination))
            .await
        {
            Ok(result) => result,
            Err(error) => {
                let _ = tokio::fs::remove_dir_all(&destination).await;
                return fail(format!("ZIP 解包任务异常结束：{error}"));
            }
        };
    if let Err(error) = extraction {
        let _ = tokio::fs::remove_dir_all(&destination).await;
        return fail(format!(
            "目录技能 {slug} 的下载内容不是安全有效的 ZIP（Content-Type: {content_type}）：{error}"
        ));
    }

    Ok(PreparedSource {
        path: destination,
        scan_subdir: String::new(),
        kind: "catalog".to_owned(),
        name,
        url: endpoint.canonical,
        reference: requested,
        version,
    })
}

async fn search_site(
    client: &Client,
    query: &str,
    endpoint: &SiteEndpoint,
) -> Result<Vec<CatalogItem>> {
    if endpoint.canonical == "https://api.skillhub.cn" {
        return search_skillhub(client, query).await;
    }
    if endpoint.canonical == "https://skills.sh" {
        return search_skills_sh(client, query).await;
    }
    let mut url = endpoint_url(&endpoint.api, &["search"])?;
    url.query_pairs_mut()
        .append_pair("q", query)
        .append_pair("limit", &SEARCH_LIMIT.to_string());
    let response = client
        .get(url.clone())
        .send()
        .await
        .map_err(|error| http_error("搜索目录", &url, error))?;
    ensure_https_response(&response, "目录搜索")?;
    let response = ensure_success("搜索目录", response).await?;
    let bytes = read_response_limited(response, MAX_JSON_BYTES, "目录搜索响应").await?;
    let envelope: SearchEnvelope = serde_json::from_slice(&bytes)
        .map_err(|error| Error::Message(format!("目录搜索返回了无效 JSON：{error}")))?;

    let mut items = Vec::with_capacity(envelope.results.len().min(SEARCH_LIMIT));
    let mut seen_slugs = HashSet::new();
    for entry in envelope.results.into_iter().take(SEARCH_LIMIT) {
        let identity = if entry.owner_handle.is_empty() {
            entry.slug.clone()
        } else {
            format!("@{}/{}", entry.owner_handle, entry.slug)
        };
        let slug = identity.trim();
        if slug.is_empty() || !seen_slugs.insert(slug.to_string()) {
            continue;
        }
        items.push(CatalogItem {
            slug: slug.to_owned(),
            name: if entry.display_name.trim().is_empty() {
                slug.to_owned()
            } else {
                entry.display_name.trim().to_owned()
            },
            description: entry.summary.trim().to_owned(),
            version: entry.version.trim().to_owned(),
            site: endpoint.canonical.clone(),
        });
    }
    Ok(items)
}

async fn prepare_github_handoff(
    cache: &Path,
    slug: &str,
    name: &str,
    catalog_site: &str,
    handoff: &GithubHandoff,
) -> Result<PreparedSource> {
    if handoff.source_ref != "public-github" {
        return fail(format!(
            "目录技能 {slug} 返回了不支持的下载来源 {:?}；当前仅支持 ZIP 和 public-github",
            handoff.source_ref
        ));
    }
    let commit = handoff.commit.trim();
    if !is_full_object_id(commit) {
        return fail(format!(
            "目录技能 {slug} 的 public-github commit 不是完整的 Git 对象 ID"
        ));
    }
    if !handoff.content_hash.is_empty()
        && !(handoff.content_hash.len() == 64
            && handoff
                .content_hash
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit()))
    {
        return fail(format!(
            "目录技能 {slug} 的 public-github contentHash 格式无效"
        ));
    }
    if !handoff.archive_url.is_empty() {
        let archive_url = Url::parse(&handoff.archive_url).map_err(|_| {
            Error::Message(format!("目录技能 {slug} 的 public-github archiveUrl 无效"))
        })?;
        if archive_url.scheme() != "https" || archive_url.host_str().is_none() {
            return fail(format!(
                "目录技能 {slug} 的 public-github archiveUrl 必须是 HTTPS URL"
            ));
        }
    }

    let repo_url = normalize_github_repo(&handoff.repo).ok_or_else(|| {
        Error::Message(format!(
            "目录技能 {slug} 的 public-github repo 不是受支持的 GitHub 仓库标识"
        ))
    })?;
    let mut prepared = prepare_git(cache, &repo_url, commit, &handoff.path).await?;
    prepared.name = name.to_owned();
    // Keep catalog identity for update checks. The canonical commit remains the
    // immutable installed version, while a refresh re-resolves the catalog slug.
    prepared.kind = "catalog".to_owned();
    prepared.url = catalog_site.to_owned();
    prepared.reference = slug.to_owned();
    Ok(prepared)
}

async fn prepare_git_inner(
    source: &ValidatedGitSource,
    reference: &str,
    subdir: &Path,
    checkout: &Path,
) -> Result<PreparedSource> {
    let checkout_arg = checkout.to_str().ok_or_else(|| {
        Error::Message(format!(
            "Git 缓存路径不是有效 UTF-8，无法安全传给 Git：{}",
            checkout.display()
        ))
    })?;
    let mut clone_args = git_base_args(source.transport);
    clone_args.extend([
        "clone".to_owned(),
        "--no-checkout".to_owned(),
        "--quiet".to_owned(),
        "--".to_owned(),
        source.url.clone(),
        checkout_arg.to_owned(),
    ]);
    let clone_output = run_git(None, &clone_args, GIT_CLONE_TIMEOUT).await?;
    require_git_success("克隆 Git 来源", &clone_output)?;

    let mut fetch_args = git_base_args(source.transport);
    fetch_args.extend([
        "fetch".to_owned(),
        "--quiet".to_owned(),
        "--tags".to_owned(),
        "origin".to_owned(),
    ]);
    let fetch_output = run_git(Some(checkout), &fetch_args, GIT_COMMAND_TIMEOUT).await?;
    require_git_success("刷新 Git 引用", &fetch_output)?;

    let mut commit = resolve_commit(checkout, source.transport, reference).await?;
    if commit.is_none() && reference != "HEAD" {
        let mut targeted_fetch = git_base_args(source.transport);
        targeted_fetch.extend([
            "fetch".to_owned(),
            "--quiet".to_owned(),
            "origin".to_owned(),
            reference.to_owned(),
        ]);
        let output = run_git(Some(checkout), &targeted_fetch, GIT_COMMAND_TIMEOUT).await?;
        require_git_success(&format!("获取 Git 引用 {reference}"), &output)?;
        commit = resolve_rev(checkout, source.transport, "FETCH_HEAD").await?;
    }
    let commit = commit
        .ok_or_else(|| Error::Message(format!("无法把 Git 引用 {reference:?} 解析为 commit")))?;
    if !is_full_object_id(&commit) {
        return fail("Git 返回了格式异常的 commit ID");
    }

    let mut checkout_args = git_base_args(source.transport);
    checkout_args.extend([
        "checkout".to_owned(),
        "--quiet".to_owned(),
        "--detach".to_owned(),
        "--force".to_owned(),
        commit.clone(),
        "--".to_owned(),
    ]);
    let checkout_output =
        run_git_isolated(Some(checkout), &checkout_args, GIT_COMMAND_TIMEOUT).await?;
    require_git_success("检出 Git 快照", &checkout_output)?;

    reject_submodules(checkout, source.transport).await?;
    reject_lfs_pointers(checkout, source.transport).await?;
    validate_checkout_subdir(checkout, subdir).await?;

    Ok(PreparedSource {
        path: checkout.to_path_buf(),
        scan_subdir: subdir.to_string_lossy().replace('\\', "/"),
        kind: "git".to_owned(),
        name: source_name(&source.url),
        url: source.url.clone(),
        reference: reference.to_owned(),
        version: commit,
    })
}

async fn resolve_commit(
    checkout: &Path,
    transport: GitTransport,
    reference: &str,
) -> Result<Option<String>> {
    let candidates = if reference == "HEAD" {
        vec!["HEAD".to_owned(), "refs/remotes/origin/HEAD".to_owned()]
    } else if reference.starts_with("refs/") || is_full_object_id(reference) {
        vec![reference.to_owned()]
    } else {
        vec![
            reference.to_owned(),
            format!("refs/remotes/origin/{reference}"),
            format!("refs/tags/{reference}"),
        ]
    };
    for candidate in candidates {
        if let Some(commit) = resolve_rev(checkout, transport, &candidate).await? {
            return Ok(Some(commit));
        }
    }
    Ok(None)
}

async fn resolve_rev(
    checkout: &Path,
    transport: GitTransport,
    revision: &str,
) -> Result<Option<String>> {
    let mut args = git_base_args(transport);
    args.extend([
        "rev-parse".to_owned(),
        "--verify".to_owned(),
        "--end-of-options".to_owned(),
        format!("{revision}^{{commit}}"),
    ]);
    let output = run_git(Some(checkout), &args, GIT_COMMAND_TIMEOUT).await?;
    if !output.status.success() {
        return Ok(None);
    }
    let commit = String::from_utf8(output.stdout)
        .map_err(|_| Error::Message("Git commit ID 不是有效 UTF-8".to_owned()))?;
    Ok(Some(commit.trim().to_ascii_lowercase()))
}

async fn reject_submodules(checkout: &Path, transport: GitTransport) -> Result<()> {
    let mut args = git_base_args(transport);
    args.extend(["ls-files".to_owned(), "--stage".to_owned(), "--".to_owned()]);
    let output = run_git(Some(checkout), &args, GIT_COMMAND_TIMEOUT).await?;
    require_git_success("检查 Git 子模块", &output)?;
    if output
        .stdout
        .split(|byte| *byte == b'\n')
        .any(|line| line.starts_with(b"160000 "))
    {
        return fail(
            "Git 来源包含子模块；SkillDock 不会自动执行或拉取子模块，请改用不依赖子模块的完整快照",
        );
    }
    Ok(())
}

async fn reject_lfs_pointers(checkout: &Path, transport: GitTransport) -> Result<()> {
    let mut args = git_base_args(transport);
    args.extend([
        "grep".to_owned(),
        "--files-with-matches".to_owned(),
        "--null".to_owned(),
        "--extended-regexp".to_owned(),
        "-e".to_owned(),
        r"^version https://git-lfs\.github\.com/spec/v1$".to_owned(),
        "--".to_owned(),
        ".".to_owned(),
    ]);
    let output = run_git(Some(checkout), &args, GIT_COMMAND_TIMEOUT).await?;
    match output.status.code() {
        Some(0) if !output.stdout.is_empty() => {
            fail("Git 来源仍包含未展开的 LFS 指针；请提供不依赖 Git LFS 的完整快照")
        }
        Some(1) => Ok(()),
        Some(0) => Ok(()),
        _ => Err(git_failure("检查 Git LFS 指针", &output)),
    }
}

async fn validate_checkout_subdir(checkout: &Path, subdir: &Path) -> Result<()> {
    if subdir.as_os_str().is_empty() {
        return Ok(());
    }
    let checkout_root = tokio::fs::canonicalize(checkout).await.map_err(|error| {
        Error::Message(format!(
            "无法解析 Git 缓存目录 {}：{error}",
            checkout.display()
        ))
    })?;
    let candidate = checkout.join(subdir);
    let metadata = tokio::fs::metadata(&candidate).await.map_err(|error| {
        Error::Message(format!(
            "Git 子目录 {} 不存在或不可访问：{error}",
            subdir.display()
        ))
    })?;
    if !metadata.is_dir() {
        return fail(format!("Git 子目录 {} 不是目录", subdir.display()));
    }
    let canonical = tokio::fs::canonicalize(&candidate).await.map_err(|error| {
        Error::Message(format!("无法解析 Git 子目录 {}：{error}", subdir.display()))
    })?;
    if !canonical.starts_with(&checkout_root) {
        return fail(format!("Git 子目录 {} 指向仓库之外", subdir.display()));
    }
    Ok(())
}

fn extract_zip(bytes: &[u8], destination: &Path) -> Result<()> {
    fs::create_dir(destination).map_err(|error| {
        Error::Message(format!(
            "无法创建解包目录 {}：{error}",
            destination.display()
        ))
    })?;
    let mut archive = ZipArchive::new(Cursor::new(bytes))
        .map_err(|error| Error::Message(format!("无法读取 ZIP：{error}")))?;
    if archive.len() > MAX_ARCHIVE_ENTRIES {
        return fail(format!(
            "ZIP 条目数 {} 超过上限 {MAX_ARCHIVE_ENTRIES}",
            archive.len()
        ));
    }

    let mut seen = HashSet::new();
    let mut extracted = 0_u64;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(|error| {
            Error::Message(format!("读取 ZIP 第 {} 个条目失败：{error}", index + 1))
        })?;
        let raw_name = entry.name();
        if raw_name.len() > MAX_ARCHIVE_PATH_BYTES
            || raw_name.contains('\0')
            || raw_name.contains('\\')
        {
            return fail(format!("ZIP 包含无效或过长的路径：{raw_name:?}"));
        }
        let raw_path = raw_name.trim_end_matches('/');
        if raw_path.is_empty()
            || raw_path
                .split('/')
                .any(|component| component.is_empty() || matches!(component, "." | ".."))
        {
            return fail(format!("ZIP 包含不安全路径分量：{raw_name:?}"));
        }
        let enclosed = entry
            .enclosed_name()
            .ok_or_else(|| Error::Message(format!("ZIP 包含越界路径：{raw_name:?}")))?;
        if enclosed.as_os_str().is_empty()
            || enclosed
                .components()
                .any(|component| !matches!(component, Component::Normal(_)))
        {
            return fail(format!("ZIP 包含不安全路径：{raw_name:?}"));
        }
        let collision_key = raw_name.trim_end_matches('/').to_lowercase();
        if !seen.insert(collision_key) {
            return fail(format!("ZIP 包含重复或大小写冲突路径：{raw_name:?}"));
        }

        let unix_mode = entry.unix_mode();
        if unix_mode.is_some_and(|mode| mode & 0o170000 == 0o120000) {
            return fail(format!("ZIP 包含符号链接，已拒绝：{raw_name:?}"));
        }
        if unix_mode.is_some_and(|mode| {
            let kind = mode & 0o170000;
            kind != 0 && kind != 0o040000 && kind != 0o100000
        }) {
            return fail(format!("ZIP 包含不支持的特殊文件：{raw_name:?}"));
        }
        if unix_mode.is_some_and(|mode| mode & 0o170000 == 0o040000) && !entry.is_dir() {
            return fail(format!("ZIP 目录条目缺少目录路径标记：{raw_name:?}"));
        }
        if unix_mode.is_some_and(|mode| mode & 0o170000 == 0o100000) && entry.is_dir() {
            return fail(format!("ZIP 文件类型与路径标记冲突：{raw_name:?}"));
        }

        let next_total = extracted
            .checked_add(entry.size())
            .ok_or_else(|| Error::Message("ZIP 展开大小溢出".to_owned()))?;
        if next_total > MAX_EXTRACTED_BYTES {
            return fail(format!(
                "ZIP 展开大小超过上限 {} MiB",
                MAX_EXTRACTED_BYTES / 1024 / 1024
            ));
        }
        let output_path = destination.join(&enclosed);
        if entry.is_dir() {
            fs::create_dir_all(&output_path).map_err(|error| {
                Error::Message(format!(
                    "创建 ZIP 目录 {} 失败：{error}",
                    output_path.display()
                ))
            })?;
            continue;
        }

        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                Error::Message(format!(
                    "创建 ZIP 父目录 {} 失败：{error}",
                    parent.display()
                ))
            })?;
        }
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&output_path)
            .map_err(|error| {
                Error::Message(format!(
                    "创建 ZIP 文件 {} 失败：{error}",
                    output_path.display()
                ))
            })?;
        let remaining = MAX_EXTRACTED_BYTES - extracted;
        let copied = std::io::copy(&mut entry.by_ref().take(remaining + 1), &mut output).map_err(
            |error| {
                Error::Message(format!(
                    "写入 ZIP 文件 {} 失败：{error}",
                    output_path.display()
                ))
            },
        )?;
        if copied > remaining {
            return fail(format!(
                "ZIP 实际展开大小超过上限 {} MiB",
                MAX_EXTRACTED_BYTES / 1024 / 1024
            ));
        }
        output.flush().map_err(|error| {
            Error::Message(format!(
                "刷新 ZIP 文件 {} 失败：{error}",
                output_path.display()
            ))
        })?;
        extracted = extracted
            .checked_add(copied)
            .ok_or_else(|| Error::Message("ZIP 实际展开大小溢出".to_owned()))?;

        #[cfg(unix)]
        if let Some(mode) = unix_mode {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&output_path, fs::Permissions::from_mode(mode & 0o777)).map_err(
                |error| {
                    Error::Message(format!(
                        "设置 ZIP 文件权限 {} 失败：{error}",
                        output_path.display()
                    ))
                },
            )?;
        }
    }
    Ok(())
}

fn validate_git_source(input: &str) -> Result<ValidatedGitSource> {
    let input = input.trim();
    if input.is_empty() {
        return fail("Git 来源不能为空");
    }
    if input.starts_with('-') || input.chars().any(char::is_control) {
        return fail("Git 来源包含不安全的选项前缀或控制字符");
    }

    let path = Path::new(input);
    if path.is_absolute() {
        if !path.exists() {
            return fail(format!("本地 Git 来源不存在：{}", path.display()));
        }
        let canonical = fs::canonicalize(path).map_err(|error| {
            Error::Message(format!("无法解析本地 Git 来源 {}：{error}", path.display()))
        })?;
        let canonical = canonical.to_str().ok_or_else(|| {
            Error::Message(format!(
                "本地 Git 来源路径不是有效 UTF-8，无法安全传给 Git：{}",
                canonical.display()
            ))
        })?;
        return Ok(ValidatedGitSource {
            url: canonical.to_owned(),
            transport: GitTransport::Local,
        });
    }

    if looks_like_scp_ssh(input) {
        return Ok(ValidatedGitSource {
            url: input.to_owned(),
            transport: GitTransport::Ssh,
        });
    }

    let parsed = Url::parse(input).map_err(|_| {
        Error::Message(
            "Git 来源必须是 HTTPS、SSH URL、scp 风格 SSH 地址或已存在的绝对本地路径".to_owned(),
        )
    })?;
    if parsed.host_str().is_none()
        || parsed.password().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
    {
        return fail("Git URL 缺少主机，或包含不允许的密码、查询参数或片段");
    }
    let transport = match parsed.scheme() {
        "https" if parsed.username().is_empty() => GitTransport::Https,
        "ssh" => GitTransport::Ssh,
        "https" => {
            return fail("HTTPS Git URL 不允许嵌入用户名或凭据，请使用 Git/SSH 的外部认证配置");
        }
        scheme => {
            return fail(format!(
                "不支持的 Git 协议 {scheme:?}；仅允许 HTTPS、SSH 和绝对本地路径"
            ));
        }
    };
    Ok(ValidatedGitSource {
        url: parsed.to_string(),
        transport,
    })
}

fn looks_like_scp_ssh(input: &str) -> bool {
    let Some((user_host, remote_path)) = input.split_once(':') else {
        return false;
    };
    let Some((user, host)) = user_host.split_once('@') else {
        return false;
    };
    if user.is_empty()
        || host.is_empty()
        || host.starts_with('-')
        || remote_path.is_empty()
        || remote_path.starts_with('-')
        || remote_path.contains('\\')
        || remote_path.split('/').any(|part| part == "..")
    {
        return false;
    }
    user.bytes().all(is_ssh_name_byte)
        && host
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
        && remote_path.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'.' | b'-' | b'_' | b'~')
        })
}

fn is_ssh_name_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_')
}

fn validate_git_reference(reference: &str) -> Result<String> {
    let reference = reference.trim();
    if reference.is_empty() {
        return Ok("HEAD".to_owned());
    }
    if reference.len() > 512
        || reference == "@"
        || reference.starts_with('-')
        || reference.starts_with('.')
        || reference.ends_with('.')
        || reference.ends_with('/')
        || reference.contains("..")
        || reference.contains("@{")
        || reference.contains("//")
        || reference.chars().any(|character| {
            character.is_control()
                || character.is_whitespace()
                || matches!(character, '~' | '^' | ':' | '?' | '*' | '[' | '\\')
        })
        || reference
            .split('/')
            .any(|part| part.is_empty() || part.starts_with('.') || part.ends_with(".lock"))
    {
        return fail(format!("Git 引用格式不安全或无效：{reference:?}"));
    }
    Ok(reference.to_owned())
}

fn validate_relative_path(path: &str, label: &str) -> Result<PathBuf> {
    let path = path.trim();
    if path.is_empty() || path == "." {
        return Ok(PathBuf::new());
    }
    if path.len() > MAX_ARCHIVE_PATH_BYTES
        || Path::new(path).is_absolute()
        || path.starts_with('/')
        || path.contains('\0')
        || path.contains('\\')
    {
        return fail(format!("{label}包含无效或过长的路径"));
    }
    let path = PathBuf::from(path.trim_end_matches('/'));
    if path.as_os_str().is_empty()
        || path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return fail(format!("{label}必须是仓库内的相对目录，且不能包含 . 或 .."));
    }
    Ok(path)
}

fn validate_slug(slug: &str) -> Result<String> {
    let slug = slug.trim();
    if slug.is_empty() {
        return fail("目录技能 slug 不能为空");
    }
    if slug.chars().count() > MAX_SLUG_CHARS
        || !slug
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        || matches!(slug.as_bytes().first(), Some(b'-' | b'_' | b'.'))
        || matches!(slug.as_bytes().last(), Some(b'-' | b'_' | b'.'))
        || slug.contains("..")
    {
        return fail(format!("目录技能 slug 格式无效：{slug:?}"));
    }
    Ok(slug.to_owned())
}

fn validate_catalog_version(version: &str) -> Result<()> {
    if version.is_empty()
        || version.len() > 128
        || version.starts_with('-')
        || version
            .chars()
            .any(|character| character.is_control() || character.is_whitespace())
    {
        return fail(format!("目录返回了无效版本号：{version:?}"));
    }
    Ok(())
}

fn normalize_site(site: &str) -> Result<SiteEndpoint> {
    let site = site.trim();
    let lowered = site.to_ascii_lowercase();
    if site.is_empty() || matches!(lowered.as_str(), "clawhub" | "clawhub.ai") {
        return site_from_url(CLAWHUB_SITE);
    }
    if matches!(
        lowered.as_str(),
        "skillhub" | "skillhub.cn" | "https://skillhub.cn" | "https://api.skillhub.cn"
    ) {
        return site_from_url("https://api.skillhub.cn");
    }
    if matches!(
        lowered.as_str(),
        "skills.sh" | "https://skills.sh" | "https://www.skills.sh"
    ) {
        return site_from_url("https://skills.sh");
    }
    site_from_url(site)
}

fn site_from_url(site: &str) -> Result<SiteEndpoint> {
    let mut root = Url::parse(site)
        .map_err(|_| Error::Message("目录站点必须是 clawhub 或有效的 HTTPS URL".to_owned()))?;
    if root.scheme() != "https"
        || root.host_str().is_none()
        || !root.username().is_empty()
        || root.password().is_some()
        || root.query().is_some()
        || root.fragment().is_some()
    {
        return fail("目录站点必须是无嵌入凭据、查询参数和片段的 HTTPS URL");
    }

    let mut path = root.path().trim_end_matches('/').to_owned();
    if path.ends_with("/api/v1") {
        path.truncate(path.len() - "/api/v1".len());
    } else if path == "/api/v1" {
        path.clear();
    }
    root.set_path(if path.is_empty() { "/" } else { &path });
    let canonical = root.as_str().trim_end_matches('/').to_owned();

    let mut api = root;
    let api_path = format!("{}/api/v1/", api.path().trim_end_matches('/'));
    api.set_path(&api_path);
    Ok(SiteEndpoint { canonical, api })
}

fn endpoint_url(base: &Url, segments: &[&str]) -> Result<Url> {
    let mut url = base.clone();
    {
        let mut path = url
            .path_segments_mut()
            .map_err(|_| Error::Message("目录站点 URL 不能作为 API 基址".to_owned()))?;
        path.pop_if_empty();
        for segment in segments {
            path.push(segment);
        }
    }
    Ok(url)
}

async fn http_client(timeout: Duration) -> Result<Client> {
    let proxy = NETWORK_PROXY.try_with(Clone::clone).unwrap_or_default();
    let resolved = crate::system_proxy::resolve(&proxy).await?;
    let bypass = reqwest::NoProxy::from_string(&resolved.bypass);
    let rule = reqwest::Proxy::custom(move |url| {
        let address = resolved.for_url(url.as_str());
        if address.is_empty() {
            None
        } else {
            Some(address.to_string())
        }
    })
    .no_proxy(bypass);
    let builder = Client::builder().no_proxy().proxy(rule);
    builder
        .user_agent(USER_AGENT)
        .connect_timeout(Duration::from_secs(10))
        .timeout(timeout)
        .pool_idle_timeout(Duration::from_secs(60))
        .redirect(reqwest::redirect::Policy::limited(5))
        .build()
        .map_err(|error| Error::Message(format!("无法创建 HTTP 客户端：{error}")))
}

fn ensure_https_response(response: &Response, label: &str) -> Result<()> {
    if response.url().scheme() != "https" {
        return fail(format!(
            "{label}被重定向到非 HTTPS 地址，已拒绝：{}",
            response.url()
        ));
    }
    Ok(())
}

async fn ensure_success(context: &str, response: Response) -> Result<Response> {
    if response.status().is_success() {
        return Ok(response);
    }
    let status = response.status();
    let retry_after = response
        .headers()
        .get(header::RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let body = read_response_limited(response, MAX_ERROR_BYTES, "错误响应")
        .await
        .unwrap_or_default();
    let message = compact_text(&String::from_utf8_lossy(&body));
    let retry = retry_after
        .map(|value| format!("，Retry-After: {value}"))
        .unwrap_or_default();
    if message.is_empty() {
        fail(format!("{context}失败：HTTP {status}{retry}"))
    } else {
        fail(format!("{context}失败：HTTP {status}{retry}，{message}"))
    }
}

async fn read_response_limited(
    mut response: Response,
    limit: usize,
    label: &str,
) -> Result<Vec<u8>> {
    if response
        .content_length()
        .is_some_and(|length| length > limit as u64)
    {
        return fail(format!("{label}大小超过上限 {} MiB", limit / 1024 / 1024));
    }
    let mut bytes =
        Vec::with_capacity(response.content_length().unwrap_or(0).min(limit as u64) as usize);
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| Error::Message(format!("读取{label}失败：{error}")))?
    {
        if bytes.len().saturating_add(chunk.len()) > limit {
            return fail(format!("{label}大小超过上限 {} MiB", limit / 1024 / 1024));
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

fn http_error(context: &str, url: &Url, error: reqwest::Error) -> Error {
    let reason = if error.is_timeout() {
        "请求超时".to_owned()
    } else if error.is_connect() {
        "连接失败".to_owned()
    } else {
        error.to_string()
    };
    Error::Message(format!("{context}失败（{}）：{reason}", safe_url(url)))
}

fn safe_url(url: &Url) -> String {
    let mut safe = url.clone();
    safe.set_query(None);
    safe.set_fragment(None);
    safe.to_string()
}

fn compact_text(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn display_site(site: &str) -> &str {
    let site = site.trim();
    if site.is_empty() { "clawhub" } else { site }
}

fn unique_cache_path(cache: &Path, prefix: &str) -> PathBuf {
    cache.join(format!("{prefix}-{}", Uuid::new_v4()))
}

fn normalize_github_repo(repo: &str) -> Option<String> {
    let repo = repo.trim().trim_end_matches('/');
    if let Some(parts) = github_repo_parts(repo) {
        return Some(format!("https://github.com/{}/{}.git", parts.0, parts.1));
    }
    let parsed = Url::parse(repo).ok()?;
    if parsed.scheme() != "https"
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
        || !parsed.host_str()?.eq_ignore_ascii_case("github.com")
    {
        return None;
    }
    let path = parsed.path().trim_matches('/').trim_end_matches(".git");
    let (owner, name) = github_repo_parts(path)?;
    Some(format!("https://github.com/{owner}/{name}.git"))
}

fn github_repo_parts(repo: &str) -> Option<(&str, &str)> {
    let repo = repo.trim_matches('/').trim_end_matches(".git");
    let mut parts = repo.split('/');
    let owner = parts.next()?;
    let name = parts.next()?;
    if parts.next().is_some()
        || owner.is_empty()
        || name.is_empty()
        || !owner.bytes().all(is_github_repo_byte)
        || !name.bytes().all(is_github_repo_byte)
    {
        return None;
    }
    Some((owner, name))
}

fn is_github_repo_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.')
}

fn source_name(url: &str) -> String {
    let path = if let Ok(parsed) = Url::parse(url) {
        parsed.path().to_owned()
    } else if let Some((_, path)) = url.split_once(':') {
        path.to_owned()
    } else {
        url.to_owned()
    };
    Path::new(path.trim_end_matches('/'))
        .file_name()
        .and_then(OsStr::to_str)
        .unwrap_or("git-source")
        .trim_end_matches(".git")
        .to_owned()
}

fn is_full_object_id(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn git_base_args(transport: GitTransport) -> Vec<String> {
    let mut args = vec![
        "-c".to_owned(),
        "protocol.allow=never".to_owned(),
        "-c".to_owned(),
        format!("core.hooksPath={}", null_device()),
        "-c".to_owned(),
        "filter.lfs.smudge=".to_owned(),
        "-c".to_owned(),
        "filter.lfs.required=false".to_owned(),
        "-c".to_owned(),
        "transfer.fsckObjects=true".to_owned(),
    ];
    let protocol = match transport {
        GitTransport::Https => "protocol.https.allow=always",
        GitTransport::Ssh => "protocol.ssh.allow=always",
        GitTransport::Local => "protocol.file.allow=always",
    };
    args.extend(["-c".to_owned(), protocol.to_owned()]);
    args
}

async fn run_git(cwd: Option<&Path>, args: &[String], timeout: Duration) -> Result<Output> {
    run_git_with_config(cwd, args, timeout, false).await
}

async fn run_git_isolated(
    cwd: Option<&Path>,
    args: &[String],
    timeout: Duration,
) -> Result<Output> {
    run_git_with_config(cwd, args, timeout, true).await
}

async fn run_git_with_config(
    cwd: Option<&Path>,
    args: &[String],
    timeout: Duration,
    isolate_config: bool,
) -> Result<Output> {
    let mut command = Command::new("git");
    let proxy = NETWORK_PROXY.try_with(Clone::clone).unwrap_or_default();
    let resolved = crate::system_proxy::resolve(&proxy).await?;
    {
        let address = &resolved.http;
        for key in [
            "HTTP_PROXY",
            "HTTPS_PROXY",
            "ALL_PROXY",
            "http_proxy",
            "https_proxy",
            "all_proxy",
            "NO_PROXY",
            "no_proxy",
        ] {
            command.env_remove(key);
        }
        command.args(["-c", &format!("http.proxy={address}")]);
        let mut urls: Vec<String> = args
            .iter()
            .filter(|a| a.starts_with("https://") || a.starts_with("http://"))
            .cloned()
            .collect();
        if let Some(path) = cwd {
            let output = Command::new("git")
                .current_dir(path)
                .args(["config", "--get", "remote.origin.url"])
                .output()
                .await?;
            if output.status.success() {
                urls.push(String::from_utf8_lossy(&output.stdout).trim().to_string());
            }
        }
        // A matching URL-specific Git setting takes precedence over http.proxy.
        for url in urls {
            if let Ok(parsed) = reqwest::Url::parse(&url) {
                if ["http", "https"].contains(&parsed.scheme()) {
                    command.args([
                        "-c",
                        &format!("http.{url}.proxy={}", resolved.for_url(&url)),
                    ]);
                }
            }
        }
        command
            .env("https_proxy", &resolved.https)
            .env("http_proxy", &resolved.http)
            .env("no_proxy", &resolved.bypass);
    }

    command
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_LFS_SKIP_SMUDGE", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    if isolate_config {
        command
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", null_device());
    }
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    let child = command
        .spawn()
        .map_err(|error| Error::Message(format!("无法启动 git：{error}；请确认系统已安装 Git")))?;
    tokio::time::timeout(timeout, child.wait_with_output())
        .await
        .map_err(|_| Error::Message(format!("Git 操作超过 {} 秒，已终止", timeout.as_secs())))?
        .map_err(|error| Error::Message(format!("等待 Git 操作失败：{error}")))
}

fn require_git_success(context: &str, output: &Output) -> Result<()> {
    if output.status.success() {
        Ok(())
    } else {
        Err(git_failure(context, output))
    }
}

fn git_failure(context: &str, output: &Output) -> Error {
    let stderr = compact_text(&String::from_utf8_lossy(&output.stderr));
    let stderr = truncate_chars(&stderr, 2_048);
    if stderr.is_empty() {
        Error::Message(format!("{context}失败：git 退出状态 {}", output.status))
    } else {
        Error::Message(format!("{context}失败：{stderr}"))
    }
}

fn truncate_chars(value: &str, limit: usize) -> String {
    let mut truncated: String = value.chars().take(limit).collect();
    if value.chars().count() > limit {
        truncated.push('…');
    }
    truncated
}

#[cfg(unix)]
fn null_device() -> &'static str {
    "/dev/null"
}

#[cfg(windows)]
fn null_device() -> &'static str {
    "NUL"
}

// Public contracts: Tencent/skillhub docs/api/skills.md and vercel-labs/skills src/find.ts.
async fn catalog_json(client: &Client, url: Url) -> Result<serde_json::Value> {
    let response = client
        .get(url.clone())
        .send()
        .await
        .map_err(|e| http_error("搜索目录", &url, e))?;
    ensure_https_response(&response, "搜索目录")?;
    let body = read_response_limited(
        ensure_success("搜索目录", response).await?,
        MAX_JSON_BYTES,
        "搜索目录",
    )
    .await?;
    serde_json::from_slice(&body).map_err(|e| Error::Message(format!("目录 JSON 无效：{e}")))
}
fn json_str<'a>(v: &'a serde_json::Value, key: &str) -> &'a str {
    v.get(key).and_then(serde_json::Value::as_str).unwrap_or("")
}
async fn search_skillhub(client: &Client, query: &str) -> Result<Vec<CatalogItem>> {
    let mut url = Url::parse("https://api.skillhub.cn/api/skills").unwrap();
    url.query_pairs_mut()
        .append_pair("keyword", query)
        .append_pair("pageSize", "50");
    let data = catalog_json(client, url).await?;
    if data.get("code").and_then(|v| v.as_i64()) != Some(0) {
        return fail("SkillHub 返回服务错误");
    }
    let entries = data
        .pointer("/data/skills")
        .and_then(|v| v.as_array())
        .ok_or_else(|| Error::Message("SkillHub 缺少结果列表".into()))?;
    Ok(entries
        .iter()
        .take(SEARCH_LIMIT)
        .filter(|v| !json_str(v, "slug").is_empty())
        .map(|v| CatalogItem {
            slug: json_str(v, "slug").into(),
            name: json_str(v, "name").into(),
            description: if json_str(v, "description_zh").is_empty() {
                json_str(v, "description")
            } else {
                json_str(v, "description_zh")
            }
            .into(),
            version: json_str(v, "version").into(),
            site: "https://api.skillhub.cn".into(),
        })
        .collect())
}
async fn search_skills_sh(client: &Client, query: &str) -> Result<Vec<CatalogItem>> {
    let mut url = Url::parse("https://skills.sh/api/search").unwrap();
    url.query_pairs_mut()
        .append_pair("q", query)
        .append_pair("limit", "20");
    let data = catalog_json(client, url).await?;
    let entries = data
        .get("skills")
        .and_then(|v| v.as_array())
        .ok_or_else(|| Error::Message("Skills.sh 缺少结果列表".into()))?;
    Ok(entries
        .iter()
        .take(SEARCH_LIMIT)
        .filter(|v| !json_str(v, "source").is_empty())
        .map(|v| CatalogItem {
            slug: format!(
                "{}@{}",
                json_str(v, "source"),
                if json_str(v, "skillId").is_empty() {
                    json_str(v, "name")
                } else {
                    json_str(v, "skillId")
                }
            ),
            name: json_str(v, "name").into(),
            description: format!("GitHub 来源：{}", json_str(v, "source")),
            version: String::new(),
            site: "https://skills.sh".into(),
        })
        .collect())
}
async fn prepare_skills_sh(cache: &Path, identity: &str) -> Result<PreparedSource> {
    let (repo, skill) = identity
        .split_once('@')
        .ok_or_else(|| Error::Message("Skills.sh 标识应为 owner/repo@skill".into()))?;
    validate_slug(skill)?;
    let url = normalize_github_repo(repo)
        .ok_or_else(|| Error::Message("Skills.sh 仓库标识无效".into()))?;
    let mut prepared = prepare_git(cache, &url, "HEAD", "").await?;
    let entries = crate::files::scan(&prepared.path)?
        .items
        .into_iter()
        .filter(|i| {
            i.status == "ready"
                && (i.name == skill
                    || Path::new(&i.path).file_name().and_then(|s| s.to_str()) == Some(skill))
        })
        .collect::<Vec<_>>();
    if entries.len() != 1 {
        return fail("仓库中的 Skill 已改名、移除或存在重名，请使用 Git 导入指定子目录");
    }
    prepared.scan_subdir = Path::new(&entries[0].path)
        .strip_prefix(&prepared.path)
        .unwrap()
        .to_string_lossy()
        .replace('\\', "/");
    prepared.kind = "catalog".into();
    prepared.url = "https://skills.sh".into();
    prepared.reference = identity.into();
    prepared.name = entries[0].name.clone();
    Ok(prepared)
}

// Request-scoped settings keep concurrent engines and connection tests isolated.
tokio::task_local! { static NETWORK_PROXY: crate::NetworkProxy; }

pub(crate) async fn with_proxy<T>(
    proxy: crate::NetworkProxy,
    task: impl std::future::Future<Output = T>,
) -> T {
    NETWORK_PROXY.scope(proxy, task).await
}

pub(crate) fn validate_proxy(proxy: &crate::NetworkProxy) -> Result<()> {
    if !["system", "inherit", "direct", "manual"].contains(&proxy.mode.as_str()) {
        return fail("无效代理模式");
    }
    if proxy.mode == "manual" {
        let url = reqwest::Url::parse(&proxy.url)
            .map_err(|_| Error::Message("请输入有效代理地址，例如 http://127.0.0.1:7897".into()))?;
        if !["http", "https"].contains(&url.scheme())
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
            || url.path() != "/"
        {
            return fail("代理仅支持 HTTP/HTTPS 地址，不支持账号密码、路径或查询参数");
        }
    }
    Ok(())
}

pub(crate) async fn test_proxy(proxy: crate::NetworkProxy) -> Result<serde_json::Value> {
    validate_proxy(&proxy)?;
    with_proxy(proxy, async {
        let resolved = crate::system_proxy::resolve(&NETWORK_PROXY.with(Clone::clone)).await?;
        let route = if resolved.https.is_empty() {
            "当前配置对 HTTPS 使用直连；若连接失败，请启用系统安全网页代理或自定义代理"
        } else {
            "已读取 HTTPS 代理"
        };
        let response = http_client(Duration::from_secs(15))
            .await?
            .get("https://github.com")
            .send()
            .await
            .map_err(|_| Error::Message(format!("HTTP 连接失败：{route}")))?;
        if !response.status().is_success() {
            return fail(format!("HTTP 测试失败：{}", response.status()));
        }
        let args = vec![
            "ls-remote".into(),
            "--exit-code".into(),
            "https://github.com/zenstory-ai/drama-skills.git".into(),
            "HEAD".into(),
        ];
        let output = run_git(None, &args, Duration::from_secs(15)).await?;
        require_git_success("Git 连接测试", &output)?;
        Ok(serde_json::json!({"message":"HTTP 与 Git 连接均成功"}))
    })
    .await
}
