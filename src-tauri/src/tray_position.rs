use tauri::{PhysicalPosition, Rect, WebviewWindow};

pub const WIDTH: f64 = 336.0;
pub const HEIGHT: f64 = 412.0;

#[derive(Clone, Copy)]
pub struct Placement {
    x: f64,
    y: f64,
    #[cfg(not(target_os = "macos"))]
    scale: f64,
}

#[derive(Clone, Copy)]
struct Bounds {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

fn placement(anchor: Bounds, area: Bounds, width: f64, height: f64, gap: f64) -> (f64, f64) {
    let x = (anchor.x + anchor.width / 2.0 - width / 2.0)
        .clamp(area.x, (area.x + area.width - width).max(area.x));
    let below = anchor.y + anchor.height + gap;
    let y = if below + height <= area.y + area.height {
        below
    } else {
        anchor.y - height - gap
    };
    (
        x,
        y.clamp(area.y, (area.y + area.height - height).max(area.y)),
    )
}

// macOS positions are global logical points. Passing physical coordinates lets Tao
// divide by the OLD window's scale, which is wrong when moving between 1x and 2x screens.
#[cfg(target_os = "macos")]
pub fn capture(
    _window: &WebviewWindow,
    rect: Rect,
    _click: PhysicalPosition<f64>,
) -> Result<Placement, String> {
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSEvent, NSScreen};
    let mtm = MainThreadMarker::new().ok_or("托盘定位需要主线程")?;
    let screens = NSScreen::screens(mtm);
    let mouse = NSEvent::mouseLocation();
    let screen = screens
        .iter()
        .find(|screen| {
            let f = screen.frame();
            mouse.x >= f.origin.x
                && mouse.x < f.origin.x + f.size.width
                && mouse.y >= f.origin.y
                && mouse.y < f.origin.y + f.size.height
        })
        .ok_or("无法确定状态栏所在屏幕")?;
    let primary = screens.firstObject().ok_or("未找到屏幕")?;
    let top = primary.frame().origin.y + primary.frame().size.height;
    let scale = screen.backingScaleFactor();
    let pos = rect.position.to_logical::<f64>(scale);
    let size = rect.size.to_logical::<f64>(scale);
    let visible = screen.visibleFrame();
    let area = Bounds {
        x: visible.origin.x,
        y: top - visible.origin.y - visible.size.height,
        width: visible.size.width,
        height: visible.size.height,
    };
    // Status-bar geometry may still describe the other display during an
    // active-display transition. Only trust it if it contains this click.
    let anchor = if contains_click(
        pos.x,
        pos.y,
        size.width,
        size.height,
        mouse.x,
        top - mouse.y,
    ) {
        Bounds {
            x: pos.x,
            y: pos.y,
            width: size.width,
            height: size.height,
        }
    } else {
        Bounds {
            x: mouse.x,
            y: top - mouse.y,
            width: 0.0,
            height: 0.0,
        }
    };
    let (x, y) = placement(anchor, area, WIDTH, HEIGHT, 4.0);
    Ok(Placement {
        x,
        y: top - y - HEIGHT,
    })
}

#[cfg(target_os = "macos")]
pub fn present(window: &WebviewWindow, placement: Placement) -> Result<(), String> {
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSWindow, NSWindowAnimationBehavior, NSWindowCollectionBehavior};
    let _mtm = MainThreadMarker::new().ok_or("托盘显示需要主线程")?;
    let pointer = window.ns_window().map_err(|e| e.to_string())?;
    // The Tauri-owned window remains alive for this main-thread operation.
    let native = unsafe { pointer.cast::<NSWindow>().as_ref() }.ok_or("菜单窗口不存在")?;
    native.setCollectionBehavior(
        NSWindowCollectionBehavior::MoveToActiveSpace
            | NSWindowCollectionBehavior::FullScreenAuxiliary
            | NSWindowCollectionBehavior::Transient
            | NSWindowCollectionBehavior::IgnoresCycle,
    );
    native.setAnimationBehavior(NSWindowAnimationBehavior::None);
    native.setOpaque(false);
    native.setHasShadow(false);
    let mut frame = native.frame();
    frame.origin.x = placement.x;
    frame.origin.y = placement.y;
    frame.size.width = WIDTH;
    frame.size.height = HEIGHT;
    native.setFrame_display(frame, true);
    // Do not call Tauri set_focus(): it activates the entire application,
    // bringing its main window/Space forward on the other display.
    native.orderFrontRegardless();
    native.makeKeyWindow();
    tracing::debug!(
        x = placement.x,
        y = placement.y,
        "tray_presented_without_app_activation"
    );
    Ok(())
}

#[cfg(any(target_os = "macos", test))]
fn contains_click(x: f64, y: f64, width: f64, height: f64, click_x: f64, click_y: f64) -> bool {
    width > 0.0
        && height > 0.0
        && click_x >= x
        && click_x <= x + width
        && click_y >= y
        && click_y <= y + height
}

#[cfg(not(target_os = "macos"))]
pub fn capture(
    window: &WebviewWindow,
    rect: Rect,
    click: PhysicalPosition<f64>,
) -> Result<Placement, String> {
    let monitor = window
        .monitor_from_point(click.x, click.y)
        .map_err(|e| e.to_string())?
        .ok_or("无法确定状态栏所在屏幕")?;
    let scale = monitor.scale_factor();
    let pos = rect.position.to_physical::<i32>(scale);
    let size = rect.size.to_physical::<u32>(scale);
    let work = monitor.work_area();
    let area = Bounds {
        x: work.position.x as f64,
        y: work.position.y as f64,
        width: work.size.width as f64,
        height: work.size.height as f64,
    };
    let anchor = if size.width > 0 && size.height > 0 {
        Bounds {
            x: pos.x as f64,
            y: pos.y as f64,
            width: size.width as f64,
            height: size.height as f64,
        }
    } else {
        Bounds {
            x: click.x,
            y: click.y,
            width: 0.0,
            height: 0.0,
        }
    };
    let (x, y) = placement(anchor, area, WIDTH * scale, HEIGHT * scale, 4.0 * scale);
    Ok(Placement { x, y, scale })
}

#[cfg(not(target_os = "macos"))]
pub fn present(window: &WebviewWindow, placement: Placement) -> Result<(), String> {
    let Placement { x, y, scale } = placement;
    window
        .set_position(PhysicalPosition::new(x.round() as i32, y.round() as i32))
        .map_err(|e| e.to_string())?;
    window
        .set_size(tauri::PhysicalSize::new(
            (WIDTH * scale).round() as u32,
            (HEIGHT * scale).round() as u32,
        ))
        .map_err(|e| e.to_string())?;
    window.show().map_err(|e| e.to_string())?;
    window.set_focus().map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stale_primary_icon_rect_is_rejected_for_secondary_click() {
        assert!(!contains_click(900., 0., 24., 24., 2100., 12.));
        assert!(contains_click(2088., 0., 24., 24., 2100., 12.));
        assert!(contains_click(-1500., -1080., 24., 24., -1488., -1068.));
    }
    #[test]
    fn top_bar_is_anchored_below_icon() {
        assert_eq!(
            placement(
                Bounds {
                    x: 900.,
                    y: 0.,
                    width: 24.,
                    height: 24.
                },
                Bounds {
                    x: 0.,
                    y: 24.,
                    width: 1440.,
                    height: 876.
                },
                WIDTH,
                HEIGHT,
                4.
            ),
            (744., 28.)
        );
    }
    #[test]
    fn negative_origin_and_right_edge_are_clamped() {
        assert_eq!(
            placement(
                Bounds {
                    x: -30.,
                    y: 0.,
                    width: 24.,
                    height: 24.
                },
                Bounds {
                    x: -1920.,
                    y: 24.,
                    width: 1920.,
                    height: 1056.
                },
                WIDTH,
                HEIGHT,
                4.
            ),
            (-336., 28.)
        );
    }
    #[test]
    fn bottom_taskbar_opens_above() {
        assert_eq!(
            placement(
                Bounds {
                    x: 1700.,
                    y: 1040.,
                    width: 24.,
                    height: 40.
                },
                Bounds {
                    x: 0.,
                    y: 0.,
                    width: 1920.,
                    height: 1040.
                },
                WIDTH,
                HEIGHT,
                4.
            ),
            (1544., 624.)
        );
    }
    #[test]
    fn physical_2x_and_logical_1x_agree() {
        let logical = placement(
            Bounds {
                x: 2000.,
                y: 0.,
                width: 24.,
                height: 24.,
            },
            Bounds {
                x: 1440.,
                y: 24.,
                width: 1920.,
                height: 1056.,
            },
            WIDTH,
            HEIGHT,
            4.,
        );
        let physical = placement(
            Bounds {
                x: 4000.,
                y: 0.,
                width: 48.,
                height: 48.,
            },
            Bounds {
                x: 2880.,
                y: 48.,
                width: 3840.,
                height: 2112.,
            },
            WIDTH * 2.,
            HEIGHT * 2.,
            8.,
        );
        assert_eq!(physical, (logical.0 * 2., logical.1 * 2.));
    }
    #[test]
    fn screen_above_primary_keeps_negative_y() {
        let (_, y) = placement(
            Bounds {
                x: 500.,
                y: -1080.,
                width: 24.,
                height: 24.,
            },
            Bounds {
                x: 0.,
                y: -1056.,
                width: 1920.,
                height: 1056.,
            },
            WIDTH,
            HEIGHT,
            4.,
        );
        assert_eq!(y, -1052.);
    }
}
