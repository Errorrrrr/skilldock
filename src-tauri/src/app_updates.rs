use crate::Runtime;
use serde_json::{Value, json};
use skilldock_core::model::Settings;
use std::{
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
    time::{Duration, Instant},
};
use tauri::Emitter;
use tauri_plugin_updater::{Update, UpdaterExt};
use tokio::sync::Mutex;

const OFFICIAL_ENDPOINT: &str =
    "https://github.com/Errorrrrr/skilldock/releases/latest/download/latest.json";
const PREVIEW_TTL: Duration = Duration::from_secs(30 * 60);

#[derive(Default)]
pub(crate) struct AppUpdates {
    pending: Mutex<Option<PendingUpdate>>,
    sequence: AtomicU64,
    pub(crate) installing: AtomicBool,
}
struct PendingUpdate {
    token: String,
    source: UpdateSource,
    network: (String, String),
    checked_at: Instant,
    update: Update,
}
#[derive(Debug, PartialEq)]
struct UpdateSource {
    endpoint: String,
    public_key: String,
    custom: bool,
}

fn resolve_source(settings: &Settings, official_key: &str) -> Result<UpdateSource, String> {
    let endpoint = settings.update_endpoint.trim();
    let public_key = settings.update_public_key.trim();
    let custom = !endpoint.is_empty() || !public_key.is_empty();
    if custom && (endpoint.is_empty() || public_key.is_empty()) {
        return Err("自定义更新源需要同时填写 HTTPS 地址和公钥，或同时清空以使用官方源".into());
    }
    let endpoint = if custom { endpoint } else { OFFICIAL_ENDPOINT };
    let url = reqwest::Url::parse(endpoint).map_err(|_| "更新地址无效")?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err("更新地址需要使用不含账号密码的 HTTPS 地址".into());
    }
    Ok(UpdateSource {
        endpoint: endpoint.into(),
        public_key: if custom {
            public_key
        } else {
            official_key.trim()
        }
        .into(),
        custom,
    })
}

// Restore the scheduler's original pause state on every failed/cancelled install.
struct InstallGuard<'a> {
    active: &'a AtomicBool,
    paused: &'a AtomicBool,
    was_paused: bool,
    installing: &'a AtomicBool,
}
impl<'a> InstallGuard<'a> {
    fn acquire(
        active: &'a AtomicBool,
        paused: &'a AtomicBool,
        installing: &'a AtomicBool,
    ) -> Result<Self, String> {
        active
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .map_err(|_| "正在执行任务，请稍后安装更新")?;
        installing.store(true, Ordering::SeqCst);
        Ok(Self {
            active,
            paused,
            installing,
            was_paused: paused.swap(true, Ordering::SeqCst),
        })
    }
}
impl Drop for InstallGuard<'_> {
    fn drop(&mut self) {
        self.paused.store(self.was_paused, Ordering::SeqCst);
        self.installing.store(false, Ordering::SeqCst);
        self.active.store(false, Ordering::SeqCst);
    }
}
fn validate_preview(
    token: &str,
    pending_token: &str,
    age: Duration,
    same_source: bool,
) -> Result<(), String> {
    if token.is_empty() || token != pending_token || age > PREVIEW_TTL || !same_source {
        return Err("更新预览已过期或更新源已变更，请重新检查更新后确认安装".into());
    }
    Ok(())
}

pub(crate) async fn execute(
    request: &Value,
    app: &tauri::AppHandle,
    state: &Runtime,
) -> Result<Value, String> {
    let action = request["action"].as_str().unwrap_or_default();
    let snapshot = state.engine.snapshot().map_err(|e| e.to_string())?;
    let official_key = app
        .config()
        .plugins
        .0
        .get("updater")
        .and_then(|p| p.get("pubkey"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    let source = resolve_source(&snapshot.settings, official_key)?;
    let configured = !source.public_key.is_empty();
    let info = json!({
        "configured": configured, "available": false,
        "currentVersion": app.package_info().version.to_string(),
        "endpoint": source.endpoint, "custom": source.custom,
        "message": if configured { "点击检查获取最新版本" } else { "此构建未包含官方更新公钥，暂不能在线升级；请安装配置了签名的正式版本" }
    });
    if action == "app_update_info" {
        return Ok(info);
    }
    let mut pending = state
        .app_updates
        .pending
        .try_lock()
        .map_err(|_| "应用更新正在处理中，请稍候")?;
    if !configured {
        *pending = None;
        if action == "install_app_update" {
            return Err("此构建尚未配置更新签名公钥".into());
        }
        return Ok(info);
    }
    if action == "check_app_update" {
        *pending = None;
        let route = skilldock_core::network::resolve_proxy(&snapshot.settings.network_proxy)
            .await
            .map_err(|e| e.to_string())?;
        let bypass = reqwest::NoProxy::from_string(&route.bypass);
        let proxy = reqwest::Proxy::custom(move |url| {
            let address = route.for_url(url.as_str());
            if address.is_empty() {
                None
            } else {
                Some(address.to_string())
            }
        })
        .no_proxy(bypass);
        let updater = app
            .updater_builder()
            .pubkey(source.public_key.clone())
            .endpoints(vec![source.endpoint.parse().map_err(|_| "更新地址无效")?])
            .map_err(|e| e.to_string())?
            .timeout(Duration::from_secs(30))
            .configure_client(move |client| {
                client
                    .no_proxy()
                    .proxy(proxy.clone())
                    .connect_timeout(Duration::from_secs(10))
            })
            .build()
            .map_err(|e| e.to_string())?;
        let mut result = info;
        match updater
            .check()
            .await
            .map_err(|e| format!("检查更新失败，请检查网络代理或发布源后重试：{e}"))?
        {
            Some(mut update) => {
                if update.download_url.scheme() != "https"
                    || !update.download_url.username().is_empty()
                    || update.download_url.password().is_some()
                {
                    return Err("发布源提供了不安全的更新包下载地址".into());
                }
                update.timeout = Some(Duration::from_secs(15 * 60));
                let token = state
                    .app_updates
                    .sequence
                    .fetch_add(1, Ordering::SeqCst)
                    .to_string();
                result["available"] = json!(true);
                result["version"] = json!(update.version);
                result["notes"] = json!(update.body.as_deref().unwrap_or_default());
                result["token"] = json!(token);
                result["message"] = json!("发现新版本");
                *pending = Some(PendingUpdate {
                    token,
                    source,
                    network: (
                        snapshot.settings.network_proxy.mode.clone(),
                        snapshot.settings.network_proxy.url.clone(),
                    ),
                    checked_at: Instant::now(),
                    update,
                });
            }
            None => result["message"] = json!("当前已是最新版本"),
        }
        return Ok(result);
    }
    if request["allowRestart"].as_bool() != Some(true) {
        return Err("请保存所有编辑，并明确确认安装重启".into());
    }
    let preview = pending.as_ref().ok_or("请先检查更新，再确认安装")?;
    validate_preview(
        request["token"].as_str().unwrap_or_default(),
        &preview.token,
        preview.checked_at.elapsed(),
        preview.source == source,
    )?;
    let _active =
        InstallGuard::acquire(&state.active, &state.paused, &state.app_updates.installing)?;
    let _files = state.engine.update_guard().map_err(|e| e.to_string())?;
    let current = state.engine.snapshot().map_err(|e| e.to_string())?;
    if current.tasks.iter().any(|t| t.status == "needsRecovery") {
        return Err("请先恢复未完成的文件事务".into());
    }
    let source_now = resolve_source(&current.settings, official_key)?;
    if source_now != preview.source
        || preview.network
            != (
                current.settings.network_proxy.mode,
                current.settings.network_proxy.url,
            )
    {
        return Err("更新配置已变更，请重新检查更新".into());
    }
    let token = &preview.token;
    let emit = |phase: &str, downloaded: u64, total: Option<u64>| {
        let _ = app.emit(
            "skilldock:app-update",
            json!({"token":token,"phase":phase,"downloaded":downloaded,"total":total}),
        );
    };
    emit("downloading", 0, None);
    let mut downloaded = 0u64;
    let mut last_event = Instant::now();
    let bytes = preview
        .update
        .download(
            |chunk, total| {
                downloaded = downloaded.saturating_add(chunk as u64);
                if last_event.elapsed() >= Duration::from_millis(100) || total == Some(downloaded) {
                    emit("downloading", downloaded, total);
                    last_event = Instant::now();
                }
            },
            || emit("verifying", 0, None),
        )
        .await
        .map_err(|e| format!("下载或签名验证失败，可以重试：{e}"))?;
    emit("installing", downloaded, Some(downloaded));
    preview
        .update
        .install(bytes)
        .map_err(|e| format!("安装失败，可以重试：{e}"))?;
    emit("restarting", downloaded, Some(downloaded));
    // Permit this authorized restart while the update holds our normal quit guards.
    state.restarting.store(true, Ordering::SeqCst);
    app.restart();
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn official_endpoint_matches_bundled_configuration() {
        let config: Value = serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        assert_eq!(
            config["plugins"]["updater"]["endpoints"][0],
            OFFICIAL_ENDPOINT
        );
    }
    #[test]
    fn official_defaults_and_custom_pairs() {
        let mut settings = Settings::default();
        let default = resolve_source(&settings, "release-key").unwrap();
        assert_eq!(default.endpoint, OFFICIAL_ENDPOINT);
        assert_eq!(default.public_key, "release-key");
        assert!(!default.custom);
        assert!(resolve_source(&settings, "").unwrap().public_key.is_empty());
        settings.update_endpoint = "https://updates.example.com/latest.json".into();
        assert!(resolve_source(&settings, "release-key").is_err());
        settings.update_public_key = "custom-key".into();
        assert_eq!(
            resolve_source(&settings, "release-key").unwrap().public_key,
            "custom-key"
        );
        for url in [
            "http://example.com",
            "https://user:password@example.com",
            "bad",
        ] {
            settings.update_endpoint = url.into();
            assert!(resolve_source(&settings, "release-key").is_err());
        }
    }
    #[test]
    fn consent_is_bound_to_preview_source_and_age() {
        assert!(validate_preview("3", "3", Duration::ZERO, true).is_ok());
        assert!(validate_preview("2", "3", Duration::ZERO, true).is_err());
        assert!(validate_preview("3", "3", Duration::from_secs(1801), true).is_err());
        assert!(validate_preview("3", "3", Duration::ZERO, false).is_err());
    }
    #[test]
    fn failed_installs_restore_pause_state_and_reject_parallel_tasks() {
        for original in [false, true] {
            let active = AtomicBool::new(false);
            let paused = AtomicBool::new(original);
            let installing = AtomicBool::new(false);
            let guard = InstallGuard::acquire(&active, &paused, &installing).unwrap();
            assert!(active.load(Ordering::SeqCst));
            assert!(paused.load(Ordering::SeqCst));
            assert!(InstallGuard::acquire(&active, &paused, &installing).is_err());
            drop(guard);
            assert!(!active.load(Ordering::SeqCst));
            assert!(!installing.load(Ordering::SeqCst));
            assert_eq!(paused.load(Ordering::SeqCst), original);
        }
    }
}
