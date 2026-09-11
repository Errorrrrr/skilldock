//! AppKit's global monitor sees other applications; its local monitor sees our
//! main window. A non-activating popup cannot rely on Focused(false) alone.
use block2::RcBlock;
use objc2::{MainThreadMarker, rc::Retained, runtime::AnyObject};
use objc2_app_kit::{NSEvent, NSEventMask, NSWindow};
use std::{cell::RefCell, ptr::NonNull};
use tauri::Manager;

struct Monitors {
    local: Retained<AnyObject>,
    global: Retained<AnyObject>,
}
impl Drop for Monitors {
    fn drop(&mut self) {
        // These tokens came from AppKit; install/uninstall run on the main thread.
        unsafe {
            NSEvent::removeMonitor(&self.local);
            NSEvent::removeMonitor(&self.global);
        }
    }
}
thread_local! {
    static MONITORS: RefCell<Option<Monitors>> = const { RefCell::new(None) };
}

pub fn install(app: &tauri::AppHandle) -> Result<(), String> {
    let _mtm = MainThreadMarker::new().ok_or("外部点击监听需要主线程")?;
    if MONITORS.with(|slot| slot.borrow().is_some()) {
        return Ok(());
    }
    let mask =
        NSEventMask::LeftMouseDown | NSEventMask::RightMouseDown | NSEventMask::OtherMouseDown;
    let handle = app.clone();
    let global_handler = RcBlock::new(move |_event: NonNull<NSEvent>| {
        crate::tray_popup::outside_click(&handle);
    });
    let global = NSEvent::addGlobalMonitorForEventsMatchingMask_handler(mask, &global_handler)
        .ok_or("无法监听其他应用的鼠标点击")?;
    let handle = app.clone();
    let local_handler = RcBlock::new(move |event: NonNull<NSEvent>| -> *mut NSEvent {
        // AppKit owns this event for the callback. Return it unchanged so the
        // original click still reaches the main window or status item.
        let event_ref = unsafe { event.as_ref() };
        let inside = handle
            .get_webview_window("tray")
            .and_then(|window| window.ns_window().ok())
            .and_then(|pointer| unsafe { pointer.cast::<NSWindow>().as_ref() })
            .is_some_and(|window| window.windowNumber() == event_ref.windowNumber());
        if !inside {
            crate::tray_popup::outside_click(&handle);
        }
        event.as_ptr()
    });
    let local =
        unsafe { NSEvent::addLocalMonitorForEventsMatchingMask_handler(mask, &local_handler) };
    let Some(local) = local else {
        unsafe {
            NSEvent::removeMonitor(&global);
        }
        return Err("无法监听应用内的鼠标点击".into());
    };
    MONITORS.with(|slot| *slot.borrow_mut() = Some(Monitors { local, global }));
    Ok(())
}

pub fn uninstall() {
    MONITORS.with(|slot| {
        slot.borrow_mut().take();
    });
}
