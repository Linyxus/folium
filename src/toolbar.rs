use std::cell::{Cell, OnceCell, RefCell};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{define_class, msg_send, sel, DefinedClass, MainThreadOnly};
use objc2_app_kit::{
    NSButton, NSGestureRecognizerState, NSImageView, NSMagnificationGestureRecognizer,
    NSModalResponseOK, NSOpenPanel, NSScreen, NSScrollView, NSTextField, NSToolbar,
    NSToolbarDelegate, NSToolbarFlexibleSpaceItemIdentifier, NSToolbarItem, NSView, NSWindow,
};
use objc2_foundation::{
    ns_string, MainThreadMarker, NSArray, NSObject, NSObjectProtocol, NSPoint, NSRect, NSSize,
    NSString, NSURL,
};
use pdfium_render::prelude::*;

use crate::pdf::{PdfStateData, RenderResult};
use crate::pdfium::get_pdfium;
use crate::ui::{rgba_to_nsimage, SendPtr};

#[derive(Debug)]
pub struct ToolbarHandlerIvars {
    image_view: OnceCell<Retained<NSImageView>>,
    page_label: OnceCell<Retained<NSTextField>>,
    pdf_state: RefCell<Option<PdfStateData>>,
    window: OnceCell<Retained<NSWindow>>,
    toolbar: OnceCell<Retained<NSToolbar>>,
    blank_view: OnceCell<Retained<NSView>>,
    pdf_view: OnceCell<Retained<NSView>>,
    scroll_view: OnceCell<Retained<NSScrollView>>,

    /// Incremented on every render request; background threads check this
    /// before posting to main queue to self-cancel when superseded.
    render_gen: Arc<AtomicUsize>,

    /// Scale at which the most recently *displayed* bitmap was rendered.
    last_rendered_scale: Cell<f32>,
}

impl Default for ToolbarHandlerIvars {
    fn default() -> Self {
        Self {
            image_view: OnceCell::new(),
            page_label: OnceCell::new(),
            pdf_state: RefCell::new(None),
            window: OnceCell::new(),
            toolbar: OnceCell::new(),
            blank_view: OnceCell::new(),
            pdf_view: OnceCell::new(),
            scroll_view: OnceCell::new(),
            render_gen: Arc::new(AtomicUsize::new(0)),
            last_rendered_scale: Cell::new(0.0),
        }
    }
}

define_class!(
    #[unsafe(super = NSObject)]
    #[thread_kind = MainThreadOnly]
    #[ivars = ToolbarHandlerIvars]
    #[name = "FoliumToolbarHandler"]
    #[derive(Debug)]
    pub struct ToolbarHandler;

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
                "new-tab" => {
                    let item = NSToolbarItem::initWithItemIdentifier(
                        NSToolbarItem::alloc(mtm),
                        item_identifier,
                    );
                    let btn = unsafe {
                        NSButton::buttonWithTitle_target_action(
                            ns_string!("+"),
                            None,
                            Some(sel!(newWindowForTab:)),
                            mtm,
                        )
                    };
                    item.setView(Some(&btn));
                    item.setLabel(ns_string!("New Tab"));
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
                ns_string!("new-tab"),
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
                ns_string!("new-tab"),
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

        #[unsafe(method(handleMagnify:))]
        fn handle_magnify(&self, recognizer: &NSMagnificationGestureRecognizer) {
            let state: NSGestureRecognizerState =
                unsafe { msg_send![recognizer, state] };
            if state != NSGestureRecognizerState::Ended {
                return;
            }
            let mag = recognizer.magnification();
            {
                let mut borrow = self.ivars().pdf_state.borrow_mut();
                if let Some(s) = borrow.as_mut() {
                    s.scale = (s.scale * (1.0 + mag as f32)).clamp(0.1, 20.0);
                }
            }
            // No setMagnification(1.0) here — render callback handles it.
            self.render_current_page();
        }
    }
);

impl ToolbarHandler {
    pub fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(ToolbarHandlerIvars::default());
        unsafe { msg_send![super(this), init] }
    }

    pub fn set_image_view(&self, image_view: Retained<NSImageView>) {
        self.ivars().image_view.set(image_view).unwrap();
    }

    pub fn set_window(&self, window: Retained<NSWindow>) {
        self.ivars().window.set(window).unwrap();
    }

    pub fn set_toolbar(&self, toolbar: Retained<NSToolbar>) {
        self.ivars().toolbar.set(toolbar).unwrap();
    }

    pub fn set_blank_view(&self, view: Retained<NSView>) {
        self.ivars().blank_view.set(view).unwrap();
    }

    pub fn set_pdf_view(&self, view: Retained<NSView>) {
        self.ivars().pdf_view.set(view).unwrap();
    }

    pub fn set_scroll_view(&self, scroll_view: Retained<NSScrollView>) {
        self.ivars().scroll_view.set(scroll_view).unwrap();
    }

    fn transition_to_pdf_view(&self, filename: &str) {
        let window = self.ivars().window.get().unwrap();
        let pdf_view = self.ivars().pdf_view.get().unwrap();
        let toolbar = self.ivars().toolbar.get().unwrap();
        window.setContentView(Some(&**pdf_view));
        window.setToolbar(Some(&**toolbar));
        window.setTitle(&NSString::from_str(filename));
    }

    fn load_url(&self, url: &NSURL) {
        let path = url.path().expect("URL has no path");
        let path_str = path.to_string();
        let bytes = std::fs::read(&path_str).expect("failed to read PDF");
        let page_count = {
            let doc = get_pdfium()
                .load_pdf_from_byte_slice(&bytes, None)
                .expect("pdfium: failed to parse PDF");
            doc.pages().len() as usize
        };
        *self.ivars().pdf_state.borrow_mut() = Some(PdfStateData {
            bytes: Arc::new(bytes),
            page_count,
            current_page: 0,
            scale: 2.0,
        });
        let filename = std::path::Path::new(&path_str)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("Document");
        self.transition_to_pdf_view(filename);
        self.render_current_page();
    }

    fn render_current_page(&self) {
        let Some(image_view) = self.ivars().image_view.get() else { return };
        let Some(scroll_view) = self.ivars().scroll_view.get() else { return };

        let mtm = MainThreadMarker::from(self);
        let backing_scale = NSScreen::mainScreen(mtm)
            .map(|s| s.backingScaleFactor())
            .unwrap_or(2.0) as f32;

        // Snapshot state on the main thread (no blocking).
        let (bytes_arc, page, page_count, scale) = {
            let borrow = self.ivars().pdf_state.borrow();
            let Some(state) = borrow.as_ref() else { return };
            (Arc::clone(&state.bytes), state.current_page, state.page_count, state.scale)
        };

        // Instant visual feedback: scale the existing bitmap while the new one renders.
        let last_scale = self.ivars().last_rendered_scale.get();
        if last_scale > 0.0 {
            scroll_view.setMagnification((scale / last_scale) as f64);
        }

        // Increment generation — invalidates all older in-flight renders.
        let render_id = self.ivars().render_gen.fetch_add(1, Ordering::SeqCst) + 1;
        let gen_arc = Arc::clone(&self.ivars().render_gen);

        // Wrap raw pointers in `SendPtr` so closures satisfy `Send`.
        // Safety: all objects are retained by TabController for the app's lifetime;
        // all dereferences happen on the main thread inside exec_async.
        let iv_ptr   = SendPtr::new(Retained::as_ptr(image_view) as *const NSImageView);
        let sv_ptr   = SendPtr::new(Retained::as_ptr(scroll_view) as *const NSScrollView);
        let pl_ptr: Option<SendPtr<NSTextField>> = self.ivars().page_label.get()
            .map(|l| SendPtr::new(Retained::as_ptr(l) as *const NSTextField));
        let self_ptr = SendPtr::new(self as *const ToolbarHandler);

        std::thread::spawn(move || {
            // --- Pdfium work on background thread ---
            let doc = get_pdfium()
                .load_pdf_from_byte_slice(&bytes_arc, None)
                .expect("pdfium: re-open failed");
            let page_ref = doc.pages().get(page as u16).expect("bad page index");

            let pt_w = page_ref.width().value * scale;
            let pt_h = page_ref.height().value * scale;
            let px_w = (pt_w * backing_scale) as i32;
            let px_h = (pt_h * backing_scale) as i32;

            let bitmap = page_ref
                .render_with_config(&PdfRenderConfig::new().set_target_size(px_w, px_h))
                .expect("pdfium render failed");

            let img = bitmap.as_image();
            let rgba = img.to_rgba8().into_raw();
            let result = RenderResult {
                rgba,
                px_w: img.width() as usize,
                px_h: img.height() as usize,
                pt_w: pt_w as f64,
                pt_h: pt_h as f64,
                scale,
                page,
                page_count,
            };
            // doc, page_ref, bitmap dropped here — pdfium resources freed

            // Pre-flight cancellation check before touching the main queue.
            if gen_arc.load(Ordering::SeqCst) != render_id { return; }

            // Post UI update to main thread.
            dispatch::Queue::main().exec_async(move || {
                if gen_arc.load(Ordering::SeqCst) != render_id { return; }

                // Safety: main thread; objects alive for app lifetime.
                let image_view  = unsafe { &*iv_ptr.as_ptr() };
                let scroll_view = unsafe { &*sv_ptr.as_ptr() };

                // Compute the fractional center of the visible area in the OLD document
                // BEFORE swapping content.  This fraction is scale-invariant and
                // will be used to restore the same page position afterwards.
                let (frac_cx, frac_cy) = {
                    let vis      = scroll_view.documentVisibleRect();
                    let doc_size = image_view.frame().size;
                    if doc_size.width > 0.0 && doc_size.height > 0.0 {
                        (
                            (vis.origin.x + vis.size.width  / 2.0) / doc_size.width,
                            (vis.origin.y + vis.size.height / 2.0) / doc_size.height,
                        )
                    } else {
                        (0.5, 0.5)
                    }
                };

                let RenderResult { mut rgba, px_w, px_h, pt_w, pt_h, scale, page, page_count } = result;
                let point_size = NSSize { width: pt_w, height: pt_h };
                let ns_image = rgba_to_nsimage(&mut rgba, px_w, px_h, point_size);

                image_view.setImage(Some(&ns_image));
                image_view.setFrame(NSRect::new(NSPoint::new(0.0, 0.0), point_size));
                scroll_view.setMagnification(1.0);

                // Restore scroll position from the fractional center.
                // Convert the fraction back to absolute doc coordinates in the NEW
                // (possibly differently-sized) document and scroll so that point
                // remains centered on screen.
                let sv_size    = scroll_view.bounds().size;
                let abs_cx     = frac_cx * pt_w;
                let abs_cy     = frac_cy * pt_h;
                let new_origin = NSPoint {
                    x: (abs_cx - sv_size.width  / 2.0).max(0.0),
                    y: (abs_cy - sv_size.height / 2.0).max(0.0),
                };
                let content_view = scroll_view.contentView();
                content_view.scrollToPoint(new_origin);
                scroll_view.reflectScrolledClipView(&content_view);

                // Record the scale this bitmap was rendered at.
                let handler = unsafe { &*self_ptr.as_ptr() };
                handler.ivars().last_rendered_scale.set(scale);

                // Update page label.
                if let Some(ref pl) = pl_ptr {
                    let label = unsafe { &*pl.as_ptr() };
                    label.setStringValue(&NSString::from_str(
                        &format!("{} of {}", page + 1, page_count)
                    ));
                }
            });
        });
    }
}
