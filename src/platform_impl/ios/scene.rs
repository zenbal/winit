//! Support for the UIKit scene-based life cycle.
//!
//! Apps built with the iOS 27 SDK must adopt it (by declaring `UIApplicationSceneManifest` in
//! their `Info.plist`), and UIKit never displays a `UIWindow` that doesn't belong to a scene.
//! Winit doesn't declare a scene delegate, so windows are attached to the application's window
//! scene here, whichever comes first: the window's creation or the scene's connection.

use std::cell::RefCell;
use std::mem;
use std::sync::OnceLock;

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{class, msg_send, msg_send_id};
use objc2_foundation::{CGRect, MainThreadMarker, NSString};
use objc2_ui_kit::UIApplication;

use super::window::WinitUIWindow;

thread_local! {
    /// Windows shown before a window scene connected.
    static PENDING_WINDOWS: RefCell<Vec<Retained<WinitUIWindow>>> = const { RefCell::new(Vec::new()) };
}

/// Whether the app opted into the scene life cycle, using the same signal as UIKit.
pub(crate) fn uses_scene_lifecycle() -> bool {
    static USES_SCENES: OnceLock<bool> = OnceLock::new();
    *USES_SCENES.get_or_init(|| {
        let key = NSString::from_str("UIApplicationSceneManifest");
        unsafe {
            let bundle: *mut AnyObject = msg_send![class!(NSBundle), mainBundle];
            let value: *mut AnyObject = msg_send![bundle, objectForInfoDictionaryKey: &*key];
            !value.is_null()
        }
    })
}

/// Make `window` key and visible, attaching it to the app's window scene first when the scene
/// life cycle is in use. Without a connected scene yet, it is shown by [`scene_will_connect`].
pub(crate) fn show_window(mtm: MainThreadMarker, window: &Retained<WinitUIWindow>) {
    if !uses_scene_lifecycle() {
        window.makeKeyAndVisible();
        return;
    }

    match connected_window_scene(mtm) {
        Some(scene) => attach(window, &scene),
        None => PENDING_WINDOWS.with(|pending| pending.borrow_mut().push(window.clone())),
    }
}

/// Handle `UISceneWillConnectNotification` by showing the windows waiting for a scene.
pub(crate) fn scene_will_connect(scene: &AnyObject) {
    if !is_window_scene(scene) {
        return;
    }

    let windows = PENDING_WINDOWS.with(|pending| mem::take(&mut *pending.borrow_mut()));
    for window in windows {
        attach(&window, scene);
    }
}

fn connected_window_scene(mtm: MainThreadMarker) -> Option<Retained<AnyObject>> {
    let app = UIApplication::sharedApplication(mtm);
    let scenes: Retained<AnyObject> = unsafe { msg_send_id![&app, connectedScenes] };
    let scene: Option<Retained<AnyObject>> = unsafe { msg_send_id![&scenes, anyObject] };
    scene.filter(|scene| is_window_scene(scene))
}

fn is_window_scene(scene: &AnyObject) -> bool {
    unsafe { msg_send![scene, isKindOfClass: class!(UIWindowScene)] }
}

fn attach(window: &WinitUIWindow, scene: &AnyObject) {
    unsafe {
        let _: () = msg_send![window, setWindowScene: scene];
        // Fill the scene, which may be smaller than the screen (iPad multitasking).
        let coordinate_space: *mut AnyObject = msg_send![scene, coordinateSpace];
        let bounds: CGRect = msg_send![coordinate_space, bounds];
        window.setFrame(bounds);
    }
    window.makeKeyAndVisible();
}
