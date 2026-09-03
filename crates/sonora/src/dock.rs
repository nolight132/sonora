#[cfg(target_os = "macos")]
pub fn show(shown: bool) {
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy};

    let Some(mtm) = MainThreadMarker::new() else {
        return log::warn!("dock: activation policy changed off the main thread");
    };
    let policy = match shown {
        true => NSApplicationActivationPolicy::Regular,
        false => NSApplicationActivationPolicy::Accessory,
    };
    NSApplication::sharedApplication(mtm).setActivationPolicy(policy);
}

#[cfg(not(target_os = "macos"))]
pub fn show(_shown: bool) {}
