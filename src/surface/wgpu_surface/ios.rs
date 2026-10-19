use objc2::{MainThreadMarker, rc::Retained};
use objc2_ui_kit::{
    UIApplication, UISceneActivationState, UIView, UIWindowScene,
    UIWindowSceneSessionRoleApplication,
};
use winit::{
    raw_window_handle::{HasWindowHandle, RawWindowHandle},
    window::Window,
};

use super::WgpuSurfaceError;

pub(super) fn prepare_window(window: &Window) -> Result<(), WgpuSurfaceError> {
    let error = |message: &str| WgpuSurfaceError::Backend(message.into());
    let mtm = MainThreadMarker::new()
        .ok_or_else(|| error("iOS window preparation requires the main thread"))?;
    let handle = window
        .window_handle()
        .map_err(|_| error("iOS window handle unavailable"))?;
    let RawWindowHandle::UiKit(handle) = handle.as_raw() else {
        return Err(error("iOS window requires a UIKit handle"));
    };
    // Winit owns the live UIView; keep it retained while UIKit updates its window.
    let view = unsafe { Retained::<UIView>::retain(handle.ui_view.as_ptr().cast()) }
        .ok_or_else(|| error("iOS window view unavailable"))?;
    let native_window = view
        .window()
        .ok_or_else(|| error("iOS view has no window"))?;
    if native_window.windowScene().is_some() {
        return Ok(());
    }

    let scenes = UIApplication::sharedApplication(mtm).connectedScenes();
    let mut selected = None;
    for scene in scenes {
        let Ok(scene) = scene.downcast::<UIWindowScene>() else {
            continue;
        };
        // This UIKit constant is available on every supported iOS version.
        let application_role = unsafe { UIWindowSceneSessionRoleApplication };
        if &*scene.session().role() != application_role
            || !matches!(
                scene.activationState(),
                UISceneActivationState::ForegroundActive
                    | UISceneActivationState::ForegroundInactive
            )
        {
            continue;
        }
        if selected.replace(scene).is_some() {
            return Err(error("iOS window has multiple eligible application scenes"));
        }
    }
    let scene = selected.ok_or_else(|| error("iOS window has no foreground application scene"))?;
    native_window.setWindowScene(Some(&scene));
    native_window.makeKeyAndVisible();
    native_window.layoutIfNeeded();
    Ok(())
}
