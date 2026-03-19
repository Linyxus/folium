use std::cell::{Cell, OnceCell, RefCell};
use std::fs::File;
use std::os::fd::{FromRawFd, IntoRawFd, RawFd};
use std::os::raw::c_void;
use std::time::SystemTime;

use dispatch::ffi::{
    dispatch_after_f, dispatch_get_main_queue, dispatch_object_t, dispatch_queue_t,
    dispatch_release, dispatch_resume, dispatch_set_context, dispatch_set_finalizer_f,
    dispatch_time, DISPATCH_TIME_NOW,
};
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{define_class, AnyThread, DefinedClass, MainThreadOnly};
use objc2_app_kit::{
    NSModalResponseOK, NSOpenPanel, NSToolbar, NSToolbarDelegate, NSToolbarItem, NSView, NSWindow,
};
use objc2_foundation::{
    ns_string, MainThreadMarker, NSArray, NSObject, NSObjectProtocol, NSString, NSURL,
};
use objc2_pdf_kit::{PDFDestination, PDFDocument};

use crate::pdf_view::FoliumPDFView;

#[repr(C)]
struct dispatch_source_type_s {
    _private: [u8; 0],
}

type DispatchSource = dispatch_object_t;
type DispatchSourceType = *const dispatch_source_type_s;

#[link(name = "System", kind = "dylib")]
unsafe extern "C" {
    static _dispatch_source_type_vnode: dispatch_source_type_s;

    fn dispatch_source_create(
        type_: DispatchSourceType,
        handle: usize,
        mask: usize,
        queue: dispatch_queue_t,
    ) -> DispatchSource;
    fn dispatch_source_set_event_handler_f(
        source: DispatchSource,
        handler: extern "C" fn(*mut c_void),
    );
    fn dispatch_source_set_cancel_handler_f(
        source: DispatchSource,
        handler: extern "C" fn(*mut c_void),
    );
    fn dispatch_source_cancel(source: DispatchSource);
}

const FILE_WATCH_EVENT_MASK: usize = 0x1 | 0x2 | 0x20 | 0x40;
const FILE_RELOAD_DEBOUNCE_NS: i64 = 250_000_000;

#[derive(Debug)]
struct FileWatchSource {
    raw: DispatchSource,
}

impl Drop for FileWatchSource {
    fn drop(&mut self) {
        unsafe {
            dispatch_source_cancel(self.raw);
            dispatch_release(self.raw);
        }
    }
}

#[derive(Debug)]
struct FileWatchContext {
    handler: *mut ToolbarHandler,
    fd: RawFd,
}

#[derive(Debug)]
struct PendingReloadContext {
    handler: Retained<ToolbarHandler>,
    generation: u64,
}

extern "C" fn file_watch_event_handler(context: *mut c_void) {
    let context = unsafe { &*(context as *mut FileWatchContext) };
    let handler = unsafe { &*context.handler };
    handler.schedule_debounced_reload();
}

extern "C" fn file_watch_cancel_handler(context: *mut c_void) {
    let context = unsafe { &mut *(context as *mut FileWatchContext) };
    if context.fd >= 0 {
        unsafe {
            drop(File::from_raw_fd(context.fd));
        }
        context.fd = -1;
    }
}

extern "C" fn file_watch_finalizer(context: *mut c_void) {
    unsafe {
        drop(Box::from_raw(context as *mut FileWatchContext));
    }
}

extern "C" fn pending_reload_fire(context: *mut c_void) {
    let context = unsafe { Box::from_raw(context as *mut PendingReloadContext) };
    if context
        .handler
        .is_reload_generation_current(context.generation)
    {
        context.handler.reload_document_if_needed();
    }
}

#[derive(Debug)]
pub struct ToolbarHandlerIvars {
    window:     OnceCell<Retained<NSWindow>>,
    blank_view: OnceCell<Retained<NSView>>,
    pdf_view:   OnceCell<Retained<FoliumPDFView>>,
    // File watcher
    watch_source: RefCell<Option<FileWatchSource>>,
    watched_path: RefCell<Option<String>>,
    file_mtime:   RefCell<Option<SystemTime>>,
    reload_generation: Cell<u64>,
}

impl Default for ToolbarHandlerIvars {
    fn default() -> Self {
        Self {
            window:     OnceCell::new(),
            blank_view: OnceCell::new(),
            pdf_view:   OnceCell::new(),
            watch_source: RefCell::new(None),
            watched_path:   RefCell::new(None),
            file_mtime:     RefCell::new(None),
            reload_generation: Cell::new(0),
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
    }
);

impl Drop for ToolbarHandler {
    fn drop(&mut self) {
        self.stop_file_watch();
    }
}

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

    fn retained_self(&self) -> Retained<Self> {
        unsafe {
            Retained::retain(self as *const Self as *mut Self)
                .expect("toolbar handler should be retainable")
        }
    }

    fn next_reload_generation(&self) -> u64 {
        let next = self.ivars().reload_generation.get().wrapping_add(1);
        self.ivars().reload_generation.set(next);
        next
    }

    fn is_reload_generation_current(&self, generation: u64) -> bool {
        self.ivars().reload_generation.get() == generation
    }

    fn schedule_debounced_reload(&self) {
        if self.ivars().watched_path.borrow().is_none() {
            return;
        }

        let generation = self.next_reload_generation();
        let context = Box::new(PendingReloadContext {
            handler: self.retained_self(),
            generation,
        });
        let when = unsafe { dispatch_time(DISPATCH_TIME_NOW, FILE_RELOAD_DEBOUNCE_NS) };
        unsafe {
            dispatch_after_f(
                when,
                dispatch_get_main_queue(),
                Box::into_raw(context) as *mut c_void,
                pending_reload_fire,
            );
        }
    }

    fn stop_file_watch(&self) {
        self.next_reload_generation();
        self.ivars().watch_source.borrow_mut().take();
        self.ivars().watched_path.borrow_mut().take();
        self.ivars().file_mtime.borrow_mut().take();
    }

    fn start_file_watch(&self, path: &str) {
        self.stop_file_watch();

        let file = match File::open(path) {
            Ok(file) => file,
            Err(_) => return,
        };
        let mtime = std::fs::metadata(path).ok().and_then(|m| m.modified().ok());
        let fd = file.into_raw_fd();
        let source = unsafe {
            dispatch_source_create(
                &_dispatch_source_type_vnode,
                fd as usize,
                FILE_WATCH_EVENT_MASK,
                dispatch_get_main_queue(),
            )
        };
        if source.is_null() {
            unsafe {
                drop(File::from_raw_fd(fd));
            }
            return;
        }

        let context = Box::new(FileWatchContext {
            handler: self as *const Self as *mut Self,
            fd,
        });
        unsafe {
            dispatch_set_context(source, Box::into_raw(context) as *mut c_void);
            dispatch_source_set_event_handler_f(source, file_watch_event_handler);
            dispatch_source_set_cancel_handler_f(source, file_watch_cancel_handler);
            dispatch_set_finalizer_f(source, file_watch_finalizer);
            dispatch_resume(source);
        }

        *self.ivars().watched_path.borrow_mut() = Some(path.to_owned());
        *self.ivars().file_mtime.borrow_mut() = mtime;
        *self.ivars().watch_source.borrow_mut() = Some(FileWatchSource { raw: source });
    }

    fn reload_document_if_needed(&self) {
        let Some(path) = self.ivars().watched_path.borrow().clone() else {
            return;
        };
        let metadata = match std::fs::metadata(&path) {
            Ok(metadata) => metadata,
            Err(_) => {
                self.stop_file_watch();
                return;
            }
        };
        let Ok(mtime) = metadata.modified() else {
            return;
        };
        if self
            .ivars()
            .file_mtime
            .borrow()
            .as_ref()
            .is_some_and(|old| *old == mtime)
        {
            return;
        }

        *self.ivars().file_mtime.borrow_mut() = Some(mtime);
        self.reload_document();
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
        self.start_file_watch(&path);
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
        let path = url.path().expect("URL has no path").to_string();

        // Load the new document.
        pv.invalidate_find_results();
        self.start_file_watch(&path);
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
