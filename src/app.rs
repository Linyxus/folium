use std::cell::RefCell;

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2::{define_class, msg_send, sel, DefinedClass, MainThreadOnly};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSApplicationDelegate, NSMenu, NSMenuItem,
    NSWindow, NSWindowDelegate, NSWindowOrderingMode,
};
use objc2_foundation::{
    ns_string, MainThreadMarker, NSNotification, NSObject, NSObjectProtocol, NSString,
};

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
            tab.window.setDelegate(Some(ProtocolObject::from_ref(self)));
            tab.window.makeKeyAndOrderFront(None);
            // Single tab on launch — hide tab bar regardless of system preference.
            self.sync_tab_bar_for(&tab.window, false);
            self.ivars().tabs.borrow_mut().push(tab);

            let app = NSApplication::sharedApplication(mtm);
            let menu = AppDelegate::build_main_menu(mtm);
            app.setMainMenu(Some(&menu));
            app.setActivationPolicy(NSApplicationActivationPolicy::Regular);
            #[allow(deprecated)]
            app.activateIgnoringOtherApps(true);
        }
    }

    unsafe impl NSWindowDelegate for AppDelegate {
        #[unsafe(method(windowWillClose:))]
        fn window_will_close(&self, notification: &NSNotification) {
            let Some(obj) = notification.object() else { return };
            let window: &NSWindow =
                unsafe { &*(Retained::as_ptr(&obj) as *const NSWindow) };

            // If dropping from 2 tabs to 1, hide the tab bar on the group.
            if let Some(tg) = window.tabGroup() {
                if tg.windows().count() == 2 && tg.isTabBarVisible() {
                    window.toggleTabBar(None);
                }
            }

            // Remove the closed tab from our tracking vec.
            let win_ptr = window as *const NSWindow;
            self.ivars().tabs.borrow_mut().retain(|t| {
                Retained::as_ptr(&t.window) != win_ptr
            });
        }
    }

    impl AppDelegate {
        #[unsafe(method(newWindowForTab:))]
        fn new_window_for_tab(&self, _sender: Option<&AnyObject>) {
            let mtm = MainThreadMarker::from(self);
            let tab = TabController::new(mtm);
            tab.window.setDelegate(Some(ProtocolObject::from_ref(self)));
            {
                let tabs = self.ivars().tabs.borrow();
                if let Some(first) = tabs.first() {
                    first.window.addTabbedWindow_ordered(
                        &tab.window,
                        NSWindowOrderingMode::Above,
                    );
                    // Now 2+ tabs — show the tab bar.
                    self.sync_tab_bar_for(&first.window, true);
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

    fn build_main_menu(mtm: MainThreadMarker) -> Retained<NSMenu> {
        let menu_bar = NSMenu::new(mtm);

        // ── App menu ──────────────────────────────────────────────
        let app_item = NSMenuItem::new(mtm);
        let app_menu = NSMenu::new(mtm);
        let quit = unsafe {
            NSMenuItem::initWithTitle_action_keyEquivalent(
                NSMenuItem::alloc(mtm),
                ns_string!("Quit Folium"),
                Some(sel!(terminate:)),
                ns_string!("q"),
            )
        };
        app_menu.addItem(&quit);
        app_item.setSubmenu(Some(&app_menu));
        menu_bar.addItem(&app_item);

        // ── File menu (macOS injects "New Tab" Cmd+T here) ───────
        let file_item = NSMenuItem::new(mtm);
        let file_menu = NSMenu::initWithTitle(NSMenu::alloc(mtm), ns_string!("File"));
        let new_tab = unsafe {
            NSMenuItem::initWithTitle_action_keyEquivalent(
                NSMenuItem::alloc(mtm),
                ns_string!("New Tab"),
                Some(sel!(newWindowForTab:)),
                ns_string!("t"),
            )
        };
        file_menu.addItem(&new_tab);
        let open = unsafe {
            NSMenuItem::initWithTitle_action_keyEquivalent(
                NSMenuItem::alloc(mtm),
                ns_string!("Open…"),
                Some(sel!(openDocument:)),
                ns_string!("o"),
            )
        };
        file_menu.addItem(&open);
        let save = unsafe {
            NSMenuItem::initWithTitle_action_keyEquivalent(
                NSMenuItem::alloc(mtm),
                ns_string!("Save"),
                Some(sel!(saveDocument:)),
                ns_string!("s"),
            )
        };
        file_menu.addItem(&save);
        let close = unsafe {
            NSMenuItem::initWithTitle_action_keyEquivalent(
                NSMenuItem::alloc(mtm),
                ns_string!("Close Tab"),
                Some(sel!(performClose:)),
                ns_string!("w"),
            )
        };
        file_menu.addItem(&close);
        file_item.setSubmenu(Some(&file_menu));
        menu_bar.addItem(&file_item);

        // ── Edit menu (enables standard text-editing shortcuts) ──
        let edit_item = NSMenuItem::new(mtm);
        let edit_menu = NSMenu::initWithTitle(NSMenu::alloc(mtm), ns_string!("Edit"));
        for (title, action, key) in [
            ("Undo",       sel!(undo:),      "z"),
            ("Redo",       sel!(redo:),       "Z"),
            ("Cut",        sel!(cut:),        "x"),
            ("Copy",       sel!(copy:),       "c"),
            ("Paste",      sel!(paste:),      "v"),
            ("Select All", sel!(selectAll:),  "a"),
        ] {
            let item = unsafe {
                NSMenuItem::initWithTitle_action_keyEquivalent(
                    NSMenuItem::alloc(mtm),
                    &NSString::from_str(title),
                    Some(action),
                    &NSString::from_str(key),
                )
            };
            edit_menu.addItem(&item);
        }
        edit_item.setSubmenu(Some(&edit_menu));
        menu_bar.addItem(&edit_item);

        // ── Window menu (macOS injects tab management here) ──────
        let win_item = NSMenuItem::new(mtm);
        let win_menu = NSMenu::initWithTitle(NSMenu::alloc(mtm), ns_string!("Window"));
        let minimize = unsafe {
            NSMenuItem::initWithTitle_action_keyEquivalent(
                NSMenuItem::alloc(mtm),
                ns_string!("Minimize"),
                Some(sel!(performMiniaturize:)),
                ns_string!("m"),
            )
        };
        win_menu.addItem(&minimize);
        win_item.setSubmenu(Some(&win_menu));
        menu_bar.addItem(&win_item);

        let app = NSApplication::sharedApplication(mtm);
        app.setWindowsMenu(Some(&win_menu));

        menu_bar
    }

    /// Ensure the tab bar for `window`'s group matches `show`.
    fn sync_tab_bar_for(&self, window: &NSWindow, show: bool) {
        if let Some(tg) = window.tabGroup() {
            let visible = tg.isTabBarVisible();
            if show && !visible {
                window.toggleTabBar(None);
            } else if !show && visible {
                window.toggleTabBar(None);
            }
        }
    }
}
