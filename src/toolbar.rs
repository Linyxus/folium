use std::cell::OnceCell;

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{define_class, sel, AnyThread, DefinedClass, MainThreadOnly};
use objc2_app_kit::{
    NSButton, NSModalResponseOK, NSOpenPanel,
    NSToolbar, NSToolbarDelegate,
    NSToolbarItem, NSView, NSWindow,
};
use objc2_foundation::{
    ns_string, MainThreadMarker, NSArray, NSObject, NSObjectProtocol, NSString, NSURL,
};
use objc2_pdf_kit::{PDFDocument, PDFView};

#[derive(Debug)]
pub struct ToolbarHandlerIvars {
    window:     OnceCell<Retained<NSWindow>>,
    toolbar:    OnceCell<Retained<NSToolbar>>,
    blank_view: OnceCell<Retained<NSView>>,
    pdf_view:   OnceCell<Retained<PDFView>>,
}

impl Default for ToolbarHandlerIvars {
    fn default() -> Self {
        Self {
            window:     OnceCell::new(),
            toolbar:    OnceCell::new(),
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
                _ => None,
            }
        }

        #[unsafe(method_id(toolbarDefaultItemIdentifiers:))]
        fn toolbarDefaultItemIdentifiers(
            &self,
            _toolbar: &NSToolbar,
        ) -> Retained<NSArray<NSString>> {
            NSArray::from_slice(&[ns_string!("new-tab")])
        }

        #[unsafe(method_id(toolbarAllowedItemIdentifiers:))]
        fn toolbarAllowedItemIdentifiers(
            &self,
            _toolbar: &NSToolbar,
        ) -> Retained<NSArray<NSString>> {
            NSArray::from_slice(&[ns_string!("new-tab")])
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

    pub fn set_toolbar(&self, toolbar: Retained<NSToolbar>) {
        self.ivars().toolbar.set(toolbar).unwrap();
    }

    pub fn set_blank_view(&self, view: Retained<NSView>) {
        self.ivars().blank_view.set(view).unwrap();
    }

    pub fn set_pdf_view(&self, view: Retained<PDFView>) {
        self.ivars().pdf_view.set(view).unwrap();
    }

    fn transition_to_pdf_view(&self, filename: &str) {
        let window   = self.ivars().window.get().unwrap();
        let pdf_view = self.ivars().pdf_view.get().unwrap();
        let toolbar  = self.ivars().toolbar.get().unwrap();
        window.setContentView(Some(&**pdf_view));
        window.setToolbar(Some(&**toolbar));
        window.setTitle(&NSString::from_str(filename));
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
