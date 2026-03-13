use std::cell::RefCell;

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{define_class, msg_send, DefinedClass, MainThreadOnly};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSApplicationDelegate, NSWindowOrderingMode,
};
use objc2_foundation::{MainThreadMarker, NSNotification, NSObject, NSObjectProtocol};

use crate::tab::TabController;

#[derive(Debug, Default)]
pub struct AppDelegateIvars {
    tabs: RefCell<Vec<TabController>>,
}

define_class!(
    #[unsafe(super = NSObject)]
    #[thread_kind = MainThreadOnly]
    #[ivars = AppDelegateIvars]
    #[name = "FoliumAppDelegate"]
    pub struct AppDelegate;

    unsafe impl NSObjectProtocol for AppDelegate {}

    unsafe impl NSApplicationDelegate for AppDelegate {
        #[unsafe(method(applicationDidFinishLaunching:))]
        fn did_finish_launching(&self, _notification: &NSNotification) {
            let mtm = MainThreadMarker::from(self);

            let tab = TabController::new(mtm);
            tab.window.makeKeyAndOrderFront(None);
            self.ivars().tabs.borrow_mut().push(tab);

            let app = NSApplication::sharedApplication(mtm);
            app.setActivationPolicy(NSApplicationActivationPolicy::Regular);
            #[allow(deprecated)]
            app.activateIgnoringOtherApps(true);
        }
    }

    impl AppDelegate {
        #[unsafe(method(newWindowForTab:))]
        fn new_window_for_tab(&self, _sender: Option<&AnyObject>) {
            let mtm = MainThreadMarker::from(self);
            let tab = TabController::new(mtm);
            {
                let tabs = self.ivars().tabs.borrow();
                if let Some(first) = tabs.first() {
                    first.window.addTabbedWindow_ordered(
                        &tab.window,
                        NSWindowOrderingMode::Above,
                    );
                }
            }
            tab.window.makeKeyAndOrderFront(None);
            self.ivars().tabs.borrow_mut().push(tab);
        }
    }
);

impl AppDelegate {
    pub fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(AppDelegateIvars::default());
        unsafe { msg_send![super(this), init] }
    }
}

