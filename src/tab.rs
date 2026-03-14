use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2::MainThreadOnly;
use objc2_app_kit::{
    NSColor, NSFont, NSTextField, NSToolbar, NSWindow, NSWindowStyleMask, NSWindowTabbingMode,
    NSWindowTitleVisibility, NSWindowToolbarStyle,
};
use objc2_foundation::{ns_string, MainThreadMarker, NSPoint, NSRect, NSSize, NSString, NSURL};

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

    pub fn load_file(&self, path: &str) {
        let url = NSURL::fileURLWithPath(&NSString::from_str(path));
        self.handler.load_url(&url);
    }
}

/// Update the ⌘N shortcut labels on all tabs based on their visual order
/// in the tab group. Call this after any tab is added or removed.
pub fn update_tab_shortcuts(window: &NSWindow, mtm: MainThreadMarker) {
    let Some(tg) = window.tabGroup() else { return };
    let windows = tg.windows();
    for i in 0..windows.count() {
        let w = windows.objectAtIndex(i);
        let tab = w.tab();
        if i < 9 {
            let text = NSString::from_str(&format!("\u{2318}{}", i + 1));
            let label = NSTextField::new(mtm);
            label.setEditable(false);
            label.setSelectable(false);
            label.setBezeled(false);
            label.setDrawsBackground(false);
            label.setStringValue(&text);
            label.setFont(Some(&NSFont::systemFontOfSize(NSFont::smallSystemFontSize())));
            label.setTextColor(Some(&NSColor::tertiaryLabelColor()));
            label.sizeToFit();
            tab.setAccessoryView(Some(&label));
        } else {
            tab.setAccessoryView(None);
        }
    }
}
