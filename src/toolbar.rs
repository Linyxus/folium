use std::cell::OnceCell;

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{define_class, AnyThread, DefinedClass, MainThreadOnly};
use objc2_app_kit::{NSModalResponseOK, NSOpenPanel, NSView, NSWindow};
use objc2_foundation::{
    ns_string, MainThreadMarker, NSArray, NSObject, NSObjectProtocol, NSString, NSURL,
};
use objc2_pdf_kit::PDFDocument;

use crate::pdf_view::FoliumPDFView;

#[derive(Debug)]
pub struct ToolbarHandlerIvars {
    window:     OnceCell<Retained<NSWindow>>,
    blank_view: OnceCell<Retained<NSView>>,
    pdf_view:   OnceCell<Retained<FoliumPDFView>>,
}

impl Default for ToolbarHandlerIvars {
    fn default() -> Self {
        Self {
            window:     OnceCell::new(),
            blank_view: OnceCell::new(),
            pdf_view:   OnceCell::new(),
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

    fn transition_to_pdf_view(&self, filename: &str) {
        let window   = self.ivars().window.get().unwrap();
        let pdf_view = self.ivars().pdf_view.get().unwrap();
        window.setContentView(Some(&**pdf_view));
        let title = NSString::from_str(filename);
        window.setTitle(&title);
        window.tab().setTitle(Some(&title));
    }

    fn load_url(&self, url: &NSURL) {
        let Some(pv) = self.ivars().pdf_view.get() else { return };
        let doc = unsafe { PDFDocument::initWithURL(PDFDocument::alloc(), url) };
        let Some(doc) = doc else { return };
        unsafe { pv.setDocument(Some(&doc)) };

        let path = url.path().expect("URL has no path").to_string();
        let filename = std::path::Path::new(&path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("Document");
        self.transition_to_pdf_view(filename);
    }
}
