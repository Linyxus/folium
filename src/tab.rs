use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2::MainThreadOnly;
use objc2_app_kit::{
    NSToolbar, NSWindow, NSWindowStyleMask, NSWindowTabbingMode, NSWindowTitleVisibility,
    NSWindowToolbarStyle,
};
use objc2_foundation::{ns_string, MainThreadMarker, NSPoint, NSRect, NSSize};

use crate::toolbar::ToolbarHandler;
use crate::ui::{build_blank_view, build_pdf_view};
use crate::window::FoliumWindow;

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
            | NSWindowStyleMask::Resizable
            | NSWindowStyleMask::FullSizeContentView;

        let frame = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(1200.0, 800.0));
        let folium_window = FoliumWindow::new(mtm, frame, style);
        let window: Retained<NSWindow> =
            unsafe { Retained::cast_unchecked(folium_window) };

        unsafe { window.setReleasedWhenClosed(false) };
        window.setTitle(ns_string!("New Tab"));
        window.setTitleVisibility(NSWindowTitleVisibility::Hidden);
        window.setTitlebarAppearsTransparent(true);
        window.setTabbingMode(NSWindowTabbingMode::Preferred);
        window.setTabbingIdentifier(ns_string!("FoliumTabGroup"));
        window.center();

        let handler = ToolbarHandler::new(mtm);

        // Minimal toolbar — triggers macOS to merge the tab bar into the titlebar.
        let toolbar = NSToolbar::initWithIdentifier(
            NSToolbar::alloc(mtm),
            ns_string!("FoliumToolbar"),
        );
        toolbar.setDelegate(Some(ProtocolObject::from_ref(&*handler)));
        window.setToolbar(Some(&*toolbar));
        window.setToolbarStyle(NSWindowToolbarStyle::UnifiedCompact);

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
