use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::MainThreadOnly;
use objc2_app_kit::{
    NSBackingStoreType, NSWindow, NSWindowStyleMask, NSWindowTabbingMode,
};
use objc2_foundation::{ns_string, MainThreadMarker, NSPoint, NSRect, NSSize};

use crate::toolbar::ToolbarHandler;
use crate::ui::{build_blank_view, build_pdf_view};

#[derive(Debug)]
pub struct TabController {
    pub window: Retained<NSWindow>,
    #[allow(dead_code)]
    handler: Retained<ToolbarHandler>,
}

impl TabController {
    pub fn new(mtm: MainThreadMarker) -> Self {
        let style = NSWindowStyleMask::Titled
            | NSWindowStyleMask::Closable
            | NSWindowStyleMask::Miniaturizable
            | NSWindowStyleMask::Resizable;

        let frame = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(1200.0, 800.0));
        let window = unsafe {
            NSWindow::initWithContentRect_styleMask_backing_defer(
                NSWindow::alloc(mtm),
                frame,
                style,
                NSBackingStoreType::Buffered,
                false,
            )
        };
        unsafe { window.setReleasedWhenClosed(false) };
        window.setTitle(ns_string!("New Tab"));
        window.setTabbingMode(NSWindowTabbingMode::Preferred);
        window.setTabbingIdentifier(ns_string!("FoliumTabGroup"));
        window.center();

        let handler = ToolbarHandler::new(mtm);
        let target: &AnyObject =
            unsafe { &*(Retained::as_ptr(&handler) as *const AnyObject) };

        let pdf_view = build_pdf_view(mtm);
        handler.set_pdf_view(pdf_view);

        let blank_view = build_blank_view(mtm, target);
        handler.set_blank_view(blank_view.clone());
        handler.set_window(window.clone());

        window.setContentView(Some(&*blank_view));

        TabController { window, handler }
    }
}
