//! AppKit's predefined Quit item calls terminate: directly. It never reaches
//! Tauri's ExitRequested handler, so use a normal menu item for Cmd+Q instead.
use tauri::{
    Manager,
    menu::{AboutMetadata, Menu, MenuItem, PredefinedMenuItem, Submenu},
};

pub const CLOSE_WINDOW: &str = "skilldock-close-window";

fn menu(app: &tauri::AppHandle) -> tauri::Result<Menu<tauri::Wry>> {
    let menu = Menu::default(app)?;
    let close = MenuItem::with_id(
        app,
        CLOSE_WINDOW,
        "关闭 SkillDock 窗口",
        true,
        Some("CmdOrCtrl+Q"),
    )?;
    let app_menu = Submenu::with_items(
        app,
        "SkillDock",
        true,
        &[
            &PredefinedMenuItem::about(
                app,
                Some("关于 SkillDock"),
                Some(AboutMetadata {
                    name: Some(app.package_info().name.clone()),
                    version: Some(app.package_info().version.to_string()),
                    ..Default::default()
                }),
            )?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::services(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::hide(app, Some("隐藏 SkillDock"))?,
            &PredefinedMenuItem::hide_others(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &close,
        ],
    )?;
    // Replace only the application submenu; retain the standard editing,
    // window and help menus (including their macOS-specific identifiers).
    menu.remove_at(0)?;
    menu.insert(&app_menu, 0)?;
    Ok(menu)
}

pub fn install(app: &tauri::AppHandle) -> tauri::Result<()> {
    app.set_menu(menu(app)?)?;
    app.on_menu_event(|app, event| close_window(app, event.id().as_ref()));
    Ok(())
}

pub fn close_window(app: &tauri::AppHandle, id: &str) {
    if id == CLOSE_WINDOW
        && let Some(window) = app.get_webview_window("main")
        && let Err(error) = window.close()
    {
        tracing::warn!(%error, "menu_close_failed");
    }
}
