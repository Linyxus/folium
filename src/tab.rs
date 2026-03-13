use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2::{sel, MainThreadOnly};
use objc2_app_kit::{
    NSBackingStoreType, NSMagnificationGestureRecognizer, NSToolbar, NSWindow, NSWindowStyleMask,
    NSWindowTabbingMode, NSWindowToolbarStyle,
};
use objc2_foundation::{ns_string, MainThreadMarker, NSPoint, NSRect, NSSize};

use crate::toolbar::ToolbarHandler;
use crate::ui::{build_blank_view, build_pdf_container};

#[derive(Debug)]
pub struct TabController {
    pub window: Retained<NSWindow>,
    // Kept alive so ObjC callbacks on the toolbar delegate remain valid.
    #[allow(dead_code)]
    handler: Retained<ToolbarHandler>,
}

impl TabController {
    pub fn new(mtm: MainThreadMarker) -> Self {
        // Build window
        let style = NSWindowStyleMask::Titled
            | NSWindowStyleMask::Closable
            | NSWindowStyleMask::Miniaturizable
            | NSWindowStyleMask::Resizable
            | NSWindowStyleMask::FullSizeContentView;

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
        window.setTitlebarAppearsTransparent(true);
        window.setTitle(ns_string!("New Tab"));
        window.setToolbarStyle(NSWindowToolbarStyle::Unified);
        window.setTabbingMode(NSWindowTabbingMode::Preferred);
        window.setTabbingIdentifier(ns_string!("FoliumTabGroup"));
        window.center();

        // Build handler
        let handler = ToolbarHandler::new(mtm);

        // Build PDF container view
        let (pdf_view, scroll_view, image_view) = build_pdf_container(mtm);
        handler.set_image_view(image_view);
        handler.set_pdf_view(pdf_view.clone());

        // Enable pinch-to-zoom on the scroll view.
        // allowsMagnification gives smooth real-time visual feedback during the
        // gesture; the gesture recognizer below triggers a crisp re-render when
        // the pinch ends.
        scroll_view.setAllowsMagnification(true);
        scroll_view.setMinMagnification(0.1);
        scroll_view.setMaxMagnification(20.0);
        handler.set_scroll_view(scroll_view.clone());

        let gr_target: &AnyObject =
            unsafe { &*(Retained::as_ptr(&handler) as *const AnyObject) };
        let gr = unsafe {
            NSMagnificationGestureRecognizer::initWithTarget_action(
                NSMagnificationGestureRecognizer::alloc(mtm),
                Some(gr_target),
                Some(sel!(handleMagnify:)),
            )
        };
        scroll_view.addGestureRecognizer(&gr);

        // Build blank "open" view
        let target: &AnyObject =
            unsafe { &*(Retained::as_ptr(&handler) as *const AnyObject) };
        let blank_view = build_blank_view(mtm, target);
        handler.set_blank_view(blank_view.clone());

        // Build toolbar (not yet attached to window — attached on first PDF load)
        let toolbar = NSToolbar::initWithIdentifier(
            NSToolbar::alloc(mtm),
            ns_string!("FoliumToolbar"),
        );
        toolbar.setDelegate(Some(ProtocolObject::from_ref(&*handler)));
        handler.set_toolbar(toolbar);

        // Wire window reference into handler
        handler.set_window(window.clone());

        // Start in blank state: no toolbar, blank content view
        window.setContentView(Some(&*blank_view));

        TabController { window, handler }
    }
}
