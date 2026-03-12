#![deny(unsafe_op_in_unsafe_fn)]
#![allow(non_snake_case)]

use std::cell::OnceCell;

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2::{define_class, msg_send, sel, AnyThread, DefinedClass, MainThreadOnly};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSApplicationDelegate, NSBackingStoreType,
    NSButton, NSLayoutConstraint, NSModalResponseOK, NSOpenPanel, NSSplitView,
    NSSplitViewDividerStyle, NSTextField, NSToolbar, NSToolbarDelegate,
    NSToolbarFlexibleSpaceItemIdentifier, NSToolbarItem, NSView, NSVisualEffectBlendingMode,
    NSVisualEffectMaterial, NSVisualEffectState, NSVisualEffectView, NSWindow, NSWindowStyleMask,
    NSWindowToolbarStyle,
};
use objc2_foundation::{
    ns_string, MainThreadMarker, NSArray, NSNotification, NSNotificationCenter, NSObject,
    NSObjectProtocol, NSPoint, NSRect, NSSize, NSString, NSURL,
};
use objc2_pdf_kit::{
    PDFDisplayMode, PDFDocument, PDFThumbnailView, PDFView, PDFViewPageChangedNotification,
};

// ---------------------------------------------------------------------------
// ToolbarHandler — toolbar delegate + action target
// ---------------------------------------------------------------------------

#[derive(Debug, Default)]
struct ToolbarHandlerIvars {
    pdf_view: OnceCell<Retained<PDFView>>,
    page_label: OnceCell<Retained<NSTextField>>,
}

define_class!(
    #[unsafe(super = NSObject)]
    #[thread_kind = MainThreadOnly]
    #[ivars = ToolbarHandlerIvars]
    #[name = "FoliumToolbarHandler"]
    #[derive(Debug)]
    struct ToolbarHandler;

    unsafe impl NSObjectProtocol for ToolbarHandler {}

    unsafe impl NSToolbarDelegate for ToolbarHandler {
        #[unsafe(method_id(toolbar:itemForItemIdentifier:willBeInsertedIntoToolbar:))]
        fn toolbar_itemForItemIdentifier_willBeInsertedIntoToolbar(
            &self,
            _toolbar: &NSToolbar,
            item_identifier: &NSString,
            _flag: bool,
        ) -> Option<Retained<NSToolbarItem>> {
            let mtm = MainThreadMarker::from(self);
            let id_str = item_identifier.to_string();
            let target: &AnyObject =
                unsafe { &*(self as *const ToolbarHandler as *const AnyObject) };

            match id_str.as_str() {
                "open" => {
                    let item = NSToolbarItem::initWithItemIdentifier(
                        NSToolbarItem::alloc(mtm),
                        item_identifier,
                    );
                    let btn = unsafe {
                        NSButton::buttonWithTitle_target_action(
                            ns_string!("Open…"),
                            Some(target),
                            Some(sel!(openDocument:)),
                            mtm,
                        )
                    };
                    item.setView(Some(&btn));
                    item.setLabel(ns_string!("Open"));
                    Some(item)
                }
                "prev" => {
                    let item = NSToolbarItem::initWithItemIdentifier(
                        NSToolbarItem::alloc(mtm),
                        item_identifier,
                    );
                    let btn = unsafe {
                        NSButton::buttonWithTitle_target_action(
                            ns_string!("‹"),
                            Some(target),
                            Some(sel!(prevPage:)),
                            mtm,
                        )
                    };
                    item.setView(Some(&btn));
                    item.setLabel(ns_string!("Previous"));
                    Some(item)
                }
                "page-label" => {
                    let item = NSToolbarItem::initWithItemIdentifier(
                        NSToolbarItem::alloc(mtm),
                        item_identifier,
                    );
                    if self.ivars().page_label.get().is_none() {
                        let label = NSTextField::labelWithString(ns_string!(""), mtm);
                        label.setTranslatesAutoresizingMaskIntoConstraints(false);
                        label
                            .widthAnchor()
                            .constraintEqualToConstant(110.0)
                            .setActive(true);
                        let _ = self.ivars().page_label.set(label);
                    }
                    let label = self.ivars().page_label.get().unwrap();
                    item.setView(Some(&**label));
                    item.setLabel(ns_string!("Page"));
                    Some(item)
                }
                "next" => {
                    let item = NSToolbarItem::initWithItemIdentifier(
                        NSToolbarItem::alloc(mtm),
                        item_identifier,
                    );
                    let btn = unsafe {
                        NSButton::buttonWithTitle_target_action(
                            ns_string!("›"),
                            Some(target),
                            Some(sel!(nextPage:)),
                            mtm,
                        )
                    };
                    item.setView(Some(&btn));
                    item.setLabel(ns_string!("Next"));
                    Some(item)
                }
                "zoom-out" => {
                    let item = NSToolbarItem::initWithItemIdentifier(
                        NSToolbarItem::alloc(mtm),
                        item_identifier,
                    );
                    let btn = unsafe {
                        NSButton::buttonWithTitle_target_action(
                            ns_string!("−"),
                            Some(target),
                            Some(sel!(zoomOut:)),
                            mtm,
                        )
                    };
                    item.setView(Some(&btn));
                    item.setLabel(ns_string!("Zoom Out"));
                    Some(item)
                }
                "zoom-in" => {
                    let item = NSToolbarItem::initWithItemIdentifier(
                        NSToolbarItem::alloc(mtm),
                        item_identifier,
                    );
                    let btn = unsafe {
                        NSButton::buttonWithTitle_target_action(
                            ns_string!("+"),
                            Some(target),
                            Some(sel!(zoomIn:)),
                            mtm,
                        )
                    };
                    item.setView(Some(&btn));
                    item.setLabel(ns_string!("Zoom In"));
                    Some(item)
                }
                _ => None,
            }
        }

        #[unsafe(method_id(toolbarDefaultItemIdentifiers:))]
        fn toolbarDefaultItemIdentifiers(
            &self,
            _toolbar: &NSToolbar,
        ) -> Retained<NSArray<NSString>> {
            let flex = unsafe { NSToolbarFlexibleSpaceItemIdentifier };
            NSArray::from_slice(&[
                ns_string!("open"),
                ns_string!("prev"),
                ns_string!("page-label"),
                ns_string!("next"),
                flex,
                ns_string!("zoom-out"),
                ns_string!("zoom-in"),
            ])
        }

        #[unsafe(method_id(toolbarAllowedItemIdentifiers:))]
        fn toolbarAllowedItemIdentifiers(
            &self,
            _toolbar: &NSToolbar,
        ) -> Retained<NSArray<NSString>> {
            let flex = unsafe { NSToolbarFlexibleSpaceItemIdentifier };
            NSArray::from_slice(&[
                ns_string!("open"),
                ns_string!("prev"),
                ns_string!("page-label"),
                ns_string!("next"),
                flex,
                ns_string!("zoom-out"),
                ns_string!("zoom-in"),
            ])
        }
    }

    // Non-protocol ObjC action methods and notification observer
    impl ToolbarHandler {
        #[unsafe(method(openDocument:))]
        fn open_document(&self, _sender: Option<&AnyObject>) {
            let mtm = MainThreadMarker::from(self);
            let panel = NSOpenPanel::openPanel(mtm);
            panel.setCanChooseFiles(true);
            panel.setCanChooseDirectories(false);
            panel.setAllowsMultipleSelection(false);
            #[allow(deprecated)]
            panel.setAllowedFileTypes(Some(&NSArray::from_slice(&[ns_string!("pdf")])));
            let result = panel.runModal();
            if result == NSModalResponseOK {
                let urls = panel.URLs();
                if let Some(url) = urls.firstObject() {
                    self.load_url(&url);
                }
            }
        }

        #[unsafe(method(prevPage:))]
        fn prev_page(&self, _sender: Option<&AnyObject>) {
            if let Some(pdf_view) = self.ivars().pdf_view.get() {
                unsafe { pdf_view.goToPreviousPage(None) };
                self.update_page_label();
            }
        }

        #[unsafe(method(nextPage:))]
        fn next_page(&self, _sender: Option<&AnyObject>) {
            if let Some(pdf_view) = self.ivars().pdf_view.get() {
                unsafe { pdf_view.goToNextPage(None) };
                self.update_page_label();
            }
        }

        #[unsafe(method(zoomIn:))]
        fn zoom_in(&self, _sender: Option<&AnyObject>) {
            if let Some(pdf_view) = self.ivars().pdf_view.get() {
                unsafe { pdf_view.zoomIn(None) };
            }
        }

        #[unsafe(method(zoomOut:))]
        fn zoom_out(&self, _sender: Option<&AnyObject>) {
            if let Some(pdf_view) = self.ivars().pdf_view.get() {
                unsafe { pdf_view.zoomOut(None) };
            }
        }

        #[unsafe(method(pageChanged:))]
        fn page_changed(&self, _notification: &NSNotification) {
            self.update_page_label();
        }
    }
);

impl ToolbarHandler {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(ToolbarHandlerIvars::default());
        unsafe { msg_send![super(this), init] }
    }

    fn load_url(&self, url: &NSURL) {
        let Some(pdf_view) = self.ivars().pdf_view.get() else {
            return;
        };
        let doc = unsafe { PDFDocument::initWithURL(PDFDocument::alloc(), url) };
        unsafe { pdf_view.setDocument(doc.as_deref()) };
        self.update_page_label();
    }

    fn update_page_label(&self) {
        let Some(pdf_view) = self.ivars().pdf_view.get() else {
            return;
        };
        let Some(page_label) = self.ivars().page_label.get() else {
            return;
        };
        let doc = unsafe { pdf_view.document() };
        let text = if let Some(doc) = &doc {
            let page = unsafe { pdf_view.currentPage() };
            let current = if let Some(page) = &page {
                (unsafe { doc.indexForPage(page) }) + 1
            } else {
                0
            };
            let total = unsafe { doc.pageCount() };
            format!("{current} of {total}")
        } else {
            String::new()
        };
        page_label.setStringValue(&NSString::from_str(&text));
    }
}

// ---------------------------------------------------------------------------
// AppDelegate
// ---------------------------------------------------------------------------

#[derive(Debug, Default)]
struct AppDelegateIvars {
    window: OnceCell<Retained<NSWindow>>,
    handler: OnceCell<Retained<ToolbarHandler>>,
}

define_class!(
    #[unsafe(super = NSObject)]
    #[thread_kind = MainThreadOnly]
    #[ivars = AppDelegateIvars]
    #[name = "FoliumAppDelegate"]
    struct AppDelegate;

    unsafe impl NSObjectProtocol for AppDelegate {}

    unsafe impl NSApplicationDelegate for AppDelegate {
        #[unsafe(method(applicationDidFinishLaunching:))]
        fn did_finish_launching(&self, _notification: &NSNotification) {
            let mtm = MainThreadMarker::from(self);

            let handler = ToolbarHandler::new(mtm);
            let window = build_window(mtm, &handler);

            window.makeKeyAndOrderFront(None);
            self.ivars().window.set(window).unwrap();
            self.ivars().handler.set(handler).unwrap();

            let app = NSApplication::sharedApplication(mtm);
            app.setActivationPolicy(NSApplicationActivationPolicy::Regular);
            #[allow(deprecated)]
            app.activateIgnoringOtherApps(true);
        }
    }
);

impl AppDelegate {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(AppDelegateIvars::default());
        unsafe { msg_send![super(this), init] }
    }
}

// ---------------------------------------------------------------------------
// Window + toolbar + content layout
// ---------------------------------------------------------------------------

fn build_window(mtm: MainThreadMarker, handler: &ToolbarHandler) -> Retained<NSWindow> {
    // --- Window ---
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
    window.setTitle(ns_string!("Folium"));
    window.setToolbarStyle(NSWindowToolbarStyle::Unified);
    window.center();

    // --- Toolbar ---
    let toolbar = NSToolbar::initWithIdentifier(
        NSToolbar::alloc(mtm),
        ns_string!("FoliumToolbar"),
    );
    toolbar.setDelegate(Some(ProtocolObject::from_ref(handler)));
    window.setToolbar(Some(&toolbar));

    // --- Root content: NSVisualEffectView ---
    let root_vev = make_visual_effect_view(
        mtm,
        NSVisualEffectMaterial::UnderWindowBackground,
        NSVisualEffectBlendingMode::BehindWindow,
    );
    root_vev.setState(NSVisualEffectState::Active);
    window.setContentView(Some(&root_vev));

    // --- PDFView (right pane) ---
    handler
        .ivars()
        .pdf_view
        .set(unsafe { PDFView::new(mtm) })
        .unwrap();
    let pdf_view = handler.ivars().pdf_view.get().unwrap();
    unsafe {
        pdf_view.setDisplayMode(PDFDisplayMode::SinglePageContinuous);
        pdf_view.setAutoScales(true);
    };
    pdf_view.setTranslatesAutoresizingMaskIntoConstraints(false);

    // --- Sidebar: NSVisualEffectView (Sidebar material) ---
    let sidebar = make_visual_effect_view(
        mtm,
        NSVisualEffectMaterial::Sidebar,
        NSVisualEffectBlendingMode::BehindWindow,
    );
    sidebar.setState(NSVisualEffectState::Active);
    sidebar.setTranslatesAutoresizingMaskIntoConstraints(false);

    // PDFThumbnailView inside sidebar
    let thumb_view = unsafe { PDFThumbnailView::new(mtm) };
    thumb_view.setTranslatesAutoresizingMaskIntoConstraints(false);
    unsafe {
        thumb_view.setThumbnailSize(NSSize {
            width: 120.0,
            height: 150.0,
        });
        thumb_view.setPDFView(Some(pdf_view));
    };
    sidebar.addSubview(&thumb_view);
    pin_to_superview(&thumb_view, &sidebar);

    // --- NSSplitView ---
    let split = NSSplitView::new(mtm);
    split.setVertical(true);
    split.setDividerStyle(NSSplitViewDividerStyle::Thin);
    split.setTranslatesAutoresizingMaskIntoConstraints(false);

    split.addArrangedSubview(&sidebar);
    split.addArrangedSubview(pdf_view);

    // Pin sidebar width
    sidebar
        .widthAnchor()
        .constraintEqualToConstant(220.0)
        .setActive(true);

    root_vev.addSubview(&split);
    pin_to_superview(&split, &root_vev);

    // --- Register for page-changed notifications ---
    let observer: &AnyObject =
        unsafe { &*(handler as *const ToolbarHandler as *const AnyObject) };
    let nc = NSNotificationCenter::defaultCenter();
    unsafe {
        nc.addObserver_selector_name_object(
            observer,
            sel!(pageChanged:),
            Some(PDFViewPageChangedNotification),
            None,
        );
    };

    window
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_visual_effect_view(
    mtm: MainThreadMarker,
    material: NSVisualEffectMaterial,
    blending: NSVisualEffectBlendingMode,
) -> Retained<NSVisualEffectView> {
    let view = NSVisualEffectView::new(mtm);
    view.setMaterial(material);
    view.setBlendingMode(blending);
    view
}

/// Pin all four edges of `view` to `superview`.
fn pin_to_superview(view: &NSView, superview: &NSView) {
    view.setTranslatesAutoresizingMaskIntoConstraints(false);
    NSLayoutConstraint::activateConstraints(&NSArray::from_retained_slice(&[
        view.topAnchor()
            .constraintEqualToAnchor(&superview.topAnchor()),
        view.bottomAnchor()
            .constraintEqualToAnchor(&superview.bottomAnchor()),
        view.leadingAnchor()
            .constraintEqualToAnchor(&superview.leadingAnchor()),
        view.trailingAnchor()
            .constraintEqualToAnchor(&superview.trailingAnchor()),
    ]));
}

// ---------------------------------------------------------------------------
// main
// ---------------------------------------------------------------------------

fn main() {
    let mtm = MainThreadMarker::new().unwrap();
    let app = NSApplication::sharedApplication(mtm);
    let delegate = AppDelegate::new(mtm);
    app.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
    app.run();
}
