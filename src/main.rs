#![deny(unsafe_op_in_unsafe_fn)]
#![allow(non_snake_case)]

use std::cell::{OnceCell, RefCell};
use std::sync::OnceLock;

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2::{define_class, msg_send, sel, AnyThread, DefinedClass, MainThreadOnly};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSApplicationDelegate, NSBackingStoreType,
    NSBitmapFormat, NSBitmapImageRep, NSButton, NSImage, NSImageScaling, NSImageView,
    NSLayoutConstraint, NSModalResponseOK, NSOpenPanel, NSScrollView, NSTextField, NSToolbar,
    NSToolbarDelegate, NSToolbarFlexibleSpaceItemIdentifier, NSToolbarItem, NSView,
    NSVisualEffectBlendingMode, NSVisualEffectMaterial, NSVisualEffectState, NSVisualEffectView,
    NSWindow, NSWindowStyleMask, NSWindowToolbarStyle,
};
use objc2_foundation::{
    ns_string, MainThreadMarker, NSArray, NSNotification, NSObject, NSObjectProtocol, NSPoint,
    NSRect, NSSize, NSString, NSURL,
};
use pdfium_render::prelude::*;

// ---------------------------------------------------------------------------
// Global Pdfium instance (single-threaded macOS app, safe to assert Send+Sync)
// ---------------------------------------------------------------------------

struct SendSyncPdfium(Pdfium);
unsafe impl Send for SendSyncPdfium {}
unsafe impl Sync for SendSyncPdfium {}

fn get_pdfium() -> &'static Pdfium {
    static INSTANCE: OnceLock<SendSyncPdfium> = OnceLock::new();
    &INSTANCE
        .get_or_init(|| {
            let dylib = std::env::current_exe()
                .expect("cannot resolve executable path")
                .parent()
                .expect("executable has no parent directory")
                .join("libpdfium.dylib");
            SendSyncPdfium(Pdfium::new(
                Pdfium::bind_to_library(&dylib).unwrap_or_else(|e| {
                    panic!("failed to load {}: {e}", dylib.display())
                }),
            ))
        })
        .0
}

// ---------------------------------------------------------------------------
// PDF state
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct PdfStateData {
    bytes: Vec<u8>,
    page_count: usize,
    current_page: usize,
    scale: f32,
}

// ---------------------------------------------------------------------------
// ToolbarHandler — toolbar delegate + action target
// ---------------------------------------------------------------------------

#[derive(Debug, Default)]
struct ToolbarHandlerIvars {
    image_view: OnceCell<Retained<NSImageView>>,
    page_label: OnceCell<Retained<NSTextField>>,
    pdf_state: RefCell<Option<PdfStateData>>,
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

    // Non-protocol ObjC action methods
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
        fn prev_page(&self, _: Option<&AnyObject>) {
            {
                let mut s = self.ivars().pdf_state.borrow_mut();
                if let Some(s) = s.as_mut() {
                    if s.current_page > 0 {
                        s.current_page -= 1;
                    } else {
                        return;
                    }
                }
            }
            self.render_current_page();
        }

        #[unsafe(method(nextPage:))]
        fn next_page(&self, _: Option<&AnyObject>) {
            {
                let mut s = self.ivars().pdf_state.borrow_mut();
                if let Some(s) = s.as_mut() {
                    if s.current_page + 1 < s.page_count {
                        s.current_page += 1;
                    } else {
                        return;
                    }
                }
            }
            self.render_current_page();
        }

        #[unsafe(method(zoomIn:))]
        fn zoom_in(&self, _: Option<&AnyObject>) {
            {
                if let Some(s) = self.ivars().pdf_state.borrow_mut().as_mut() {
                    s.scale *= 1.25;
                }
            }
            self.render_current_page();
        }

        #[unsafe(method(zoomOut:))]
        fn zoom_out(&self, _: Option<&AnyObject>) {
            {
                if let Some(s) = self.ivars().pdf_state.borrow_mut().as_mut() {
                    s.scale /= 1.25;
                }
            }
            self.render_current_page();
        }
    }
);

impl ToolbarHandler {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(ToolbarHandlerIvars::default());
        unsafe { msg_send![super(this), init] }
    }

    fn load_url(&self, url: &NSURL) {
        let path = url.path().expect("URL has no path");
        let bytes = std::fs::read(path.to_string()).expect("failed to read PDF");
        let page_count = {
            let doc = get_pdfium()
                .load_pdf_from_byte_slice(&bytes, None)
                .expect("pdfium: failed to parse PDF");
            doc.pages().len() as usize
        };
        *self.ivars().pdf_state.borrow_mut() = Some(PdfStateData {
            bytes,
            page_count,
            current_page: 0,
            scale: 2.0,
        });
        self.render_current_page();
    }

    fn update_page_label(&self) {
        let Some(label) = self.ivars().page_label.get() else {
            return;
        };
        let text = match self.ivars().pdf_state.borrow().as_ref() {
            Some(s) => format!("{} of {}", s.current_page + 1, s.page_count),
            None => String::new(),
        };
        label.setStringValue(&NSString::from_str(&text));
    }

    fn render_current_page(&self) {
        let Some(image_view) = self.ivars().image_view.get() else {
            return;
        };

        // Extract pixel data inside a block so that `borrow`, `doc`, `page`, and
        // `bitmap` are all dropped before we call `update_page_label` (which would
        // re-borrow `pdf_state`).
        let (mut rgba, w, h) = {
            let borrow = self.ivars().pdf_state.borrow();
            let Some(state) = borrow.as_ref() else {
                return;
            };

            let doc = get_pdfium()
                .load_pdf_from_byte_slice(&state.bytes, None)
                .expect("pdfium: re-open failed");
            let page = doc.pages().get(state.current_page as u16).expect("bad page index");

            let px_w = (page.width().value * state.scale) as i32;
            let px_h = (page.height().value * state.scale) as i32;
            let bitmap = page
                .render_with_config(&PdfRenderConfig::new().set_target_size(px_w, px_h))
                .expect("pdfium render failed");

            let img = bitmap.as_image();
            let w = img.width() as usize;
            let h = img.height() as usize;
            let rgba = img.to_rgba8().into_raw();
            (rgba, w, h)
            // page, doc, borrow all dropped here
        };

        let ns_image = rgba_to_nsimage(&mut rgba, w, h);
        image_view.setImage(Some(&ns_image));
        image_view.setFrame(NSRect::new(
            NSPoint::new(0.0, 0.0),
            NSSize {
                width: w as f64,
                height: h as f64,
            },
        ));
        self.update_page_label();
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

    // --- NSScrollView fills the content area ---
    let scroll = NSScrollView::new(mtm);
    scroll.setHasHorizontalScroller(true);
    scroll.setHasVerticalScroller(true);
    scroll.setDrawsBackground(false);
    root_vev.addSubview(&scroll);
    pin_to_superview(&scroll, &root_vev);

    // --- NSImageView as document view (frame-based, not Auto Layout) ---
    // NSImageScaling(2) == NSImageScaleNone
    let image_view = NSImageView::new(mtm);
    image_view.setImageScaling(NSImageScaling(2));
    scroll.setDocumentView(Some(&*image_view));

    handler.ivars().image_view.set(image_view).unwrap();

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

/// Pin all four edges of `view` to `superview` using Auto Layout.
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

/// Convert a raw RGBA buffer into an `NSImage` via `NSBitmapImageRep`.
/// `NSBitmapImageRep` copies the pixel data during init, so `rgba` can be
/// dropped after this function returns.
fn rgba_to_nsimage(rgba: &mut Vec<u8>, w: usize, h: usize) -> Retained<NSImage> {
    unsafe {
        // The init method expects a pointer to a C array of plane pointers.
        // For packed (non-planar) RGBA we provide a single pointer.
        let mut plane: *mut u8 = rgba.as_mut_ptr();
        let planes: *mut *mut u8 = &raw mut plane;

        let rep = NSBitmapImageRep::initWithBitmapDataPlanes_pixelsWide_pixelsHigh_bitsPerSample_samplesPerPixel_hasAlpha_isPlanar_colorSpaceName_bitmapFormat_bytesPerRow_bitsPerPixel(
            NSBitmapImageRep::alloc(),
            planes,
            w as isize,
            h as isize,
            8,               // bitsPerSample
            4,               // samplesPerPixel (RGBA)
            true,            // hasAlpha
            false,           // isPlanar
            objc2_app_kit::NSDeviceRGBColorSpace,
            NSBitmapFormat(0),
            (w * 4) as isize, // bytesPerRow
            32,              // bitsPerPixel
        )
        .expect("NSBitmapImageRep init failed");

        let img = NSImage::initWithSize(
            NSImage::alloc(),
            NSSize {
                width: w as f64,
                height: h as f64,
            },
        );
        img.addRepresentation(&rep);
        img
    }
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
