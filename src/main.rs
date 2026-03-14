#![deny(unsafe_op_in_unsafe_fn)]
#![allow(non_snake_case)]

mod app;
mod pdf_view;
mod tab;
mod toolbar;
mod ui;
mod window;

use objc2::runtime::ProtocolObject;
use objc2_app_kit::NSApplication;
use objc2_foundation::MainThreadMarker;

use app::AppDelegate;

/// CLI file paths to open on launch, set before `app.run()`.
pub static CLI_PATHS: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();

fn main() {
    let paths: Vec<String> = std::env::args().skip(1).collect();
    let _ = CLI_PATHS.set(paths);

    let mtm = MainThreadMarker::new().unwrap();
    let app = NSApplication::sharedApplication(mtm);
    let delegate = AppDelegate::new(mtm);
    app.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
    app.run();
}
