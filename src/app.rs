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

use crate::tab::{self, TabController};

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
            let app = NSApplication::sharedApplication(mtm);
            let menu = AppDelegate::build_main_menu(mtm);
            app.setMainMenu(Some(&menu));
            app.setActivationPolicy(NSApplicationActivationPolicy::Regular);
            #[allow(deprecated)]
            app.activateIgnoringOtherApps(true);

            let cli_paths = crate::CLI_PATHS.get().cloned().unwrap_or_default();
            if cli_paths.is_empty() {
                let tab = TabController::new(mtm);
                tab.window.setDelegate(Some(ProtocolObject::from_ref(self)));
                tab.window.makeKeyAndOrderFront(None);
                tab::update_tab_shortcuts(&tab.window, mtm);
                self.ivars().tabs.borrow_mut().push(tab);
            } else {
                for (i, path) in cli_paths.iter().enumerate() {
                    let tab = TabController::new(mtm);
                    tab.window.setDelegate(Some(ProtocolObject::from_ref(self)));
                    tab.load_file(path);
                    if i == 0 {
                        tab.window.makeKeyAndOrderFront(None);
                    } else {
                        let tabs = self.ivars().tabs.borrow();
                        if let Some(first) = tabs.first() {
                            first.window.addTabbedWindow_ordered(
                                &tab.window,
                                NSWindowOrderingMode::Above,
                            );
                        }
                        tab.window.makeKeyAndOrderFront(None);
                    }
                    self.ivars().tabs.borrow_mut().push(tab);
                }
                // Set shortcuts based on actual tab group order.
                let tabs = self.ivars().tabs.borrow();
                if let Some(first) = tabs.first() {
                    tab::update_tab_shortcuts(&first.window, mtm);
                }
            }
        }
    }

    unsafe impl NSWindowDelegate for AppDelegate {
        #[unsafe(method(windowWillClose:))]
        fn window_will_close(&self, notification: &NSNotification) {
            let Some(obj) = notification.object() else { return };
            let window: &NSWindow =
                unsafe { &*(Retained::as_ptr(&obj) as *const NSWindow) };

            // Remove the closed tab from our tracking vec.
            let win_ptr = window as *const NSWindow;
            self.ivars().tabs.borrow_mut().retain(|t| {
                Retained::as_ptr(&t.window) != win_ptr
            });

            // Re-number remaining tab shortcuts.
            let mtm = MainThreadMarker::from(self);
            let tabs = self.ivars().tabs.borrow();
            if let Some(first) = tabs.first() {
                tab::update_tab_shortcuts(&first.window, mtm);
            }
        }
    }

    impl AppDelegate {
        #[unsafe(method(selectTabByIndex:))]
        fn select_tab_by_index(&self, sender: Option<&AnyObject>) {
            let Some(sender) = sender else { return };
            let tag: i64 = unsafe { msg_send![sender, tag] };
            let mtm = MainThreadMarker::from(self);
            let app = NSApplication::sharedApplication(mtm);
            let Some(window) = app.keyWindow() else { return };
            let Some(tg) = window.tabGroup() else { return };
            let windows = tg.windows();
            let idx = tag as usize;
            if idx < windows.count() {
                let target = windows.objectAtIndex(idx);
                target.makeKeyAndOrderFront(None);
            }
        }

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
                }
            }
            tab.window.makeKeyAndOrderFront(None);
            self.ivars().tabs.borrow_mut().push(tab);

            // Re-number all tab shortcuts based on visual order.
            let tabs = self.ivars().tabs.borrow();
            if let Some(first) = tabs.first() {
                tab::update_tab_shortcuts(&first.window, mtm);
            }
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
        win_menu.addItem(&NSMenuItem::separatorItem(mtm));
        for i in 1..=9u8 {
            let title = NSString::from_str(&format!("Tab {}", i));
            let key = NSString::from_str(&format!("{}", i));
            let item = unsafe {
                NSMenuItem::initWithTitle_action_keyEquivalent(
                    NSMenuItem::alloc(mtm),
                    &title,
                    Some(sel!(selectTabByIndex:)),
                    &key,
                )
            };
            item.setTag((i - 1) as isize);
            win_menu.addItem(&item);
        }
        win_item.setSubmenu(Some(&win_menu));
        menu_bar.addItem(&win_item);

        let app = NSApplication::sharedApplication(mtm);
        app.setWindowsMenu(Some(&win_menu));

        menu_bar
    }

}
