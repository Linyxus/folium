#![deny(unsafe_op_in_unsafe_fn)]
#![allow(non_snake_case)]

mod app;
mod pdf;
mod pdfium;
mod tab;
mod toolbar;
mod ui;

use objc2::runtime::ProtocolObject;
use objc2_app_kit::NSApplication;
use objc2_foundation::MainThreadMarker;

use app::AppDelegate;

fn main() {
    pdfium::init_pdfium();
    let mtm = MainThreadMarker::new().unwrap();
    let app = NSApplication::sharedApplication(mtm);
    let delegate = AppDelegate::new(mtm);
    app.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
    app.run();
}
