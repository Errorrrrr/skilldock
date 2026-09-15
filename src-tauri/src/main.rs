#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod app_updates;
mod directories;
#[cfg(target_os = "macos")]
mod tray_outside;
mod tray_popup;
mod tray_position;
use serde_json::{Value, json};
use skilldock_core::Engine;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tauri::{
    Emitter, Manager, WebviewUrl, WebviewWindowBuilder,
    tray::{MouseButtonState, TrayIconBuilder, TrayIconEvent},
};
use tauri_plugin_notification::NotificationExt;

struct Runtime {
    engine: Engine,
    paused: Arc<AtomicBool>,
    tray_available: bool,
    active: Arc<AtomicBool>,
    app_updates: app_updates::AppUpdates,
    restarting: AtomicBool,
}

#[tauri::command]
async fn execute(
    request: Value,
    app: tauri::AppHandle,
    state: tauri::State<'_, Runtime>,
) -> Result<Value, String> {
    let action = request.get("action").and_then(Value::as_str).unwrap_or("");
    if matches!(
        action,
        "app_update_info" | "check_app_update" | "install_app_update"
    ) {
        app_updates::execute(&request, &app, &state).await
    } else {
        let engine = state.engine.clone();
        let handle = tokio::runtime::Handle::current();
        let result =
            tauri::async_runtime::spawn_blocking(move || handle.block_on(engine.execute(request)))
                .await
                .map_err(|e| e.to_string())?
                .map_err(|e| e.to_string());
        if result.is_ok() {
            let _ = app.emit("skilldock:changed", ());
        }
        result
    }
}
#[tauri::command]
async fn tray_action(
    action: String,
    app: tauri::AppHandle,
    state: tauri::State<'_, Runtime>,
) -> Result<Value, String> {
    match action.as_str() {
        "status" => {}
        "ready" | "hide" => {
            let handle = app.clone();
            let is_ready = action == "ready";
            app.run_on_main_thread(move || {
                if is_ready {
                    tray_popup::ready(&handle);
                } else {
                    tray_popup::hide(&handle);
                }
            })
            .map_err(|e| e.to_string())?;
        }
        "open" | "configure" => {
            let handle = app.clone();
            let configure = action == "configure";
            app.run_on_main_thread(move || {
                tray_popup::hide(&handle);
                show(&handle);
                if configure && let Some(w) = handle.get_webview_window("main") {
                    let _ = w.emit("skilldock:navigate-updates", ());
                }
            })
            .map_err(|e| e.to_string())?;
        }
        "pause" => {
            if state.app_updates.installing.load(Ordering::SeqCst) {
                return Err("正在安装应用更新，请稍后调整定时任务".into());
            }
            let previous = state.paused.fetch_xor(true, Ordering::SeqCst);
            let _ = app.emit("skilldock:scheduler", json!({"paused": !previous}));
        }
        "check" => {
            if state.active.swap(true, Ordering::SeqCst) {
                return Err("更新正在进行，请稍候".into());
            }
            let mut errors = Vec::new();
            match state.engine.snapshot() {
                Ok(snapshot) => {
                    for source in snapshot.sources {
                        if !source.supports_remote_updates() {
                            continue;
                        }
                        if let Err(error) = state.engine.check_source(&source.id, false).await {
                            errors.push(format!("{}：{}", source.name, error));
                        }
                    }
                }
                Err(error) => errors.push(error.to_string()),
            }
            state.active.store(false, Ordering::SeqCst);
            let _ = app.emit("skilldock:changed", ());
            if !errors.is_empty() {
                return Err(errors.join("；"));
            }
        }
        "quit" => {
            if state.active.load(Ordering::SeqCst) {
                return Err("更新正在进行，请稍后退出".into());
            }
            let guard = state
                .engine
                .update_guard()
                .map_err(|_| "正在写入文件，请稍后退出".to_string())?;
            drop(guard);
            app.exit(0);
        }
        _ => return Err("未知菜单操作".into()),
    }
    let snapshot = state.engine.snapshot().map_err(|e| e.to_string())?;
    Ok(
        json!({"paused":state.paused.load(Ordering::SeqCst), "active":state.active.load(Ordering::SeqCst),
        "skills":snapshot.skills.iter().map(|skill| &skill.name).collect::<std::collections::BTreeSet<_>>().len(), "sources":snapshot.sources.iter().filter(|source| source.supports_remote_updates()).count(),
        "scheduled":snapshot.sources.iter().filter(|s| s.supports_remote_updates() && (s.policy.mode == "notify" || s.policy.mode == "auto")).count(),
        "automatic":snapshot.sources.iter().filter(|s| s.supports_remote_updates() && s.policy.mode == "auto").count(),
        "theme":snapshot.settings.theme}),
    )
}
fn show(app: &tauri::AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}
fn main() {
    tracing_subscriber::fmt().with_env_filter("info").init();
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| show(app)))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_autostart::Builder::new().build())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(
            tauri_plugin_window_state::Builder::default()
                .with_denylist(&["tray"])
                .build(),
        )
        .setup(|app| {
            let test_config = if cfg!(debug_assertions) {
                std::env::var_os("SKILLDOCK_TEST_CONFIG").map(std::path::PathBuf::from)
            } else {
                None
            };
            let engine = Engine::new(test_config)?;
            let paused = Arc::new(AtomicBool::new(false));
            let active = Arc::new(AtomicBool::new(false));
            app.manage(tray_popup::Popup::default());
            WebviewWindowBuilder::new(app, "tray", WebviewUrl::App("index.html#/tray".into()))
                .title("SkillDock 快捷菜单")
                .inner_size(tray_position::WIDTH, tray_position::HEIGHT)
                .decorations(false)
                .resizable(false)
                .always_on_top(true)
                .skip_taskbar(true)
                .visible(false)
                .focused(false)
                .shadow(false)
                .transparent(true)
                .build()?;
            #[cfg(target_os = "macos")]
            if let Err(error) = tray_outside::install(app.handle()) {
                tracing::warn!(%error, "tray_outside_monitor_failed");
            }
            let mut tray = TrayIconBuilder::new()
                .tooltip("SkillDock · Skill 管理")
                .show_menu_on_left_click(false)
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        position,
                        rect,
                        button_state: MouseButtonState::Down,
                        ..
                    } = event
                    {
                        tray_popup::click(tray.app_handle(), rect, position);
                    }
                });
            if let Some(icon) = app.default_window_icon() {
                tray = tray.icon(icon.clone());
            }
            // Linux tray click events are unavailable in Tauri; keep the main window recoverable.
            let tray_available = !cfg!(target_os = "linux") && tray.build(app).is_ok();
            tracing::info!(tray_available, "desktop_ready");
            app.manage(Runtime {
                engine: engine.clone(),
                paused: paused.clone(),
                tray_available,
                active: active.clone(),
                app_updates: app_updates::AppUpdates::default(),
                restarting: AtomicBool::new(false),
            });
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                loop {
                    tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                    if paused.load(Ordering::SeqCst) || active.swap(true, Ordering::SeqCst) {
                        continue;
                    }
                    let before = engine.snapshot().ok();
                    let _ = engine.run_due_updates().await;
                    if let Ok(after) = engine.snapshot() {
                        let changed = after
                            .sources
                            .iter()
                            .filter(|source| {
                                source.supports_remote_updates()
                                    && ["available", "attention", "error"]
                                        .contains(&source.status.as_str())
                                    && before.as_ref().is_some_and(|b| {
                                        b.sources.iter().any(|old| {
                                            old.id == source.id
                                                && (old.status != source.status
                                                    || old.error != source.error)
                                        })
                                    })
                            })
                            .count();
                        if changed > 0 {
                            let _ = handle
                                .notification()
                                .builder()
                                .title("SkillDock 来源更新")
                                .body(format!("{changed} 个来源有新变化，请打开更新中心查看。"))
                                .show();
                        }
                    }
                    active.store(false, Ordering::SeqCst);
                    let _ = handle.emit("skilldock:changed", ());
                }
            });
            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() == "tray" {
                match event {
                    tauri::WindowEvent::Focused(false) => {
                        tray_popup::blur(window.app_handle());
                    }
                    tauri::WindowEvent::CloseRequested { api, .. } => {
                        api.prevent_close();
                        tray_popup::hide(window.app_handle());
                    }
                    _ => {}
                }
                return;
            }
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let app = window.app_handle();
                let rt = app.state::<Runtime>();
                if rt.tray_available
                    && rt
                        .engine
                        .snapshot()
                        .map(|s| s.settings.close_to_tray)
                        .unwrap_or(true)
                {
                    api.prevent_close();
                    tracing::info!("window_hidden_to_tray");
                    let _ = window.hide();
                } else {
                    // The hidden tray webview must not keep the app alive after a normal close.
                    api.prevent_close();
                    if rt.active.load(Ordering::SeqCst) || rt.engine.update_guard().is_err() {
                        let _ = app.emit("skilldock:notice", "正在执行任务，请稍后退出");
                    } else {
                        app.exit(0);
                    }
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            execute,
            tray_action,
            directories::open_directory
        ])
        .build(tauri::generate_context!())
        .expect("无法启动 SkillDock")
        .run(|app, event| {
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Exit = event {
                tray_outside::uninstall();
            }
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Reopen { .. } = event {
                show(app);
            }
            if let tauri::RunEvent::ExitRequested { api, .. } = event {
                let rt = app.state::<Runtime>();
                if !rt.restarting.load(Ordering::SeqCst)
                    && (rt.active.load(Ordering::SeqCst) || rt.engine.update_guard().is_err())
                {
                    api.prevent_exit();
                    show(app);
                }
            }
        });
}
