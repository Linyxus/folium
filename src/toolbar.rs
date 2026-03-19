use std::cell::{OnceCell, RefCell};
use std::time::SystemTime;

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{define_class, msg_send, sel, AnyThread, DefinedClass, MainThreadOnly};
use objc2_app_kit::{
    NSModalResponseOK, NSOpenPanel, NSToolbar, NSToolbarDelegate, NSToolbarItem, NSView, NSWindow,
};
use objc2_foundation::{
    ns_string, MainThreadMarker, NSArray, NSObject, NSObjectProtocol, NSString, NSURL,
};
use objc2_pdf_kit::{PDFDestination, PDFDocument};

use crate::pdf_view::FoliumPDFView;

#[derive(Debug)]
pub struct ToolbarHandlerIvars {
    window:     OnceCell<Retained<NSWindow>>,
    blank_view: OnceCell<Retained<NSView>>,
    pdf_view:   OnceCell<Retained<FoliumPDFView>>,
    // File watcher
    watched_path: RefCell<Option<String>>,
    file_mtime:   RefCell<Option<SystemTime>>,
    watcher_active: RefCell<bool>,
}

impl Default for ToolbarHandlerIvars {
    fn default() -> Self {
        Self {
            window:     OnceCell::new(),
            blank_view: OnceCell::new(),
            pdf_view:   OnceCell::new(),
            watched_path:   RefCell::new(None),
            file_mtime:     RefCell::new(None),
            watcher_active: RefCell::new(false),
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
            Some(NSToolbarItem::initWithItemIdentifier(
                NSToolbarItem::alloc(mtm),
                item_identifier,
            ))
        }

        #[unsafe(method_id(toolbarDefaultItemIdentifiers:))]
        fn toolbarDefaultItemIdentifiers(
            &self,
            _toolbar: &NSToolbar,
        ) -> Retained<NSArray<NSString>> {
            NSArray::from_slice(&[ns_string!("NSToolbarFlexibleSpaceItem")])
        }

        #[unsafe(method_id(toolbarAllowedItemIdentifiers:))]
        fn toolbarAllowedItemIdentifiers(
            &self,
            _toolbar: &NSToolbar,
        ) -> Retained<NSArray<NSString>> {
            NSArray::from_slice(&[ns_string!("NSToolbarFlexibleSpaceItem")])
        }
    }

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

        #[unsafe(method(_checkFileChanged:))]
        fn _check_file_changed(&self, _sender: Option<&AnyObject>) {
            if let Some(path) = self.ivars().watched_path.borrow().as_ref() {
                if let Ok(meta) = std::fs::metadata(path) {
                    if let Ok(mtime) = meta.modified() {
                        let changed = self
                            .ivars()
                            .file_mtime
                            .borrow()
                            .map_or(false, |old| mtime != old);
                        if changed {
                            *self.ivars().file_mtime.borrow_mut() = Some(mtime);
                            self.reload_document();
                        }
                    }
                }
            }
            // Re-schedule in 1 second.
            self.schedule_file_check();
        }
    }
);

impl ToolbarHandler {
    pub fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(ToolbarHandlerIvars::default());
        unsafe { objc2::msg_send![super(this), init] }
    }

    pub fn set_window(&self, window: Retained<NSWindow>) {
        self.ivars().window.set(window).unwrap();
    }

    pub fn set_blank_view(&self, view: Retained<NSView>) {
        self.ivars().blank_view.set(view).unwrap();
    }

    pub fn set_pdf_view(&self, view: Retained<FoliumPDFView>) {
        self.ivars().pdf_view.set(view).unwrap();
    }

    pub fn has_document(&self) -> bool {
        self.ivars()
            .pdf_view
            .get()
            .and_then(|pv| unsafe { pv.document() })
            .is_some()
    }

    fn transition_to_pdf_view(&self, filename: &str) {
        let window   = self.ivars().window.get().unwrap();
        let pdf_view = self.ivars().pdf_view.get().unwrap();
        window.setContentView(Some(&**pdf_view));
        let title = NSString::from_str(filename);
        window.setTitle(&title);
        window.tab().setTitle(Some(&title));
    }

    fn schedule_file_check(&self) {
        let self_ptr = self as *const ToolbarHandler as *const AnyObject;
        let null: *const AnyObject = std::ptr::null();
        unsafe {
            let _: () = msg_send![
                &*self_ptr,
                performSelector: sel!(_checkFileChanged:),
                withObject: null,
                afterDelay: 1.0_f64
            ];
        }
    }

    pub fn load_url(&self, url: &NSURL) {
        let Some(pv) = self.ivars().pdf_view.get() else { return };
        pv.invalidate_find_results();
        let doc = unsafe { PDFDocument::initWithURL(PDFDocument::alloc(), url) };
        let Some(doc) = doc else { return };
        unsafe { pv.setDocument(Some(&doc)) };

        let path = url.path().expect("URL has no path").to_string();
        let filename = std::path::Path::new(&path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("Document");
        self.transition_to_pdf_view(filename);

        // Start file watching.
        let mtime = std::fs::metadata(&path).ok().and_then(|m| m.modified().ok());
        *self.ivars().watched_path.borrow_mut() = Some(path);
        *self.ivars().file_mtime.borrow_mut() = mtime;
        if !*self.ivars().watcher_active.borrow() {
            *self.ivars().watcher_active.borrow_mut() = true;
            self.schedule_file_check();
        }
    }

    fn reload_document(&self) {
        let Some(pv) = self.ivars().pdf_view.get() else { return };
        let doc = unsafe { pv.document() };
        let Some(doc) = doc else { return };
        let url = unsafe { doc.documentURL() };
        let Some(url) = url else { return };

        // Save state.
        let scale = unsafe { pv.scaleFactor() };
        let destination = unsafe { pv.currentDestination() };
        let page_index = destination.as_ref().and_then(|dest| {
            let page = unsafe { dest.page() }?;
            Some(unsafe { doc.indexForPage(&page) })
        });

        // Load the new document.
        pv.invalidate_find_results();
        let new_doc = unsafe { PDFDocument::initWithURL(PDFDocument::alloc(), &url) };
        let Some(new_doc) = new_doc else { return };
        unsafe { pv.setDocument(Some(&new_doc)) };

        // Restore state.
        unsafe { pv.setScaleFactor(scale) };
        if let (Some(dest), Some(idx)) = (&destination, page_index) {
            let new_page_count = unsafe { new_doc.pageCount() };
            let target_idx = idx.min(new_page_count.saturating_sub(1));
            if let Some(new_page) = unsafe { new_doc.pageAtIndex(target_idx) } {
                let point = unsafe { dest.point() };
                let new_dest = unsafe {
                    PDFDestination::initWithPage_atPoint(
                        PDFDestination::alloc(),
                        &new_page,
                        point,
                    )
                };
                unsafe { pv.goToDestination(&new_dest) };
            }
        }
    }
}
