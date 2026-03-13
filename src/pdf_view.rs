use std::cell::{OnceCell, RefCell};

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{define_class, msg_send, sel, AnyThread, DefinedClass, MainThreadOnly};
use objc2_app_kit::{
    NSAlert, NSBackingStoreType, NSBezelStyle, NSButton, NSColor, NSEvent, NSImage, NSMenu,
    NSMenuItem, NSPanel, NSScrollView, NSTextField, NSTextView, NSTrackingArea,
    NSTrackingAreaOptions, NSWindowStyleMask,
};
use objc2_foundation::{
    ns_string, MainThreadMarker, NSNotification, NSNotificationCenter, NSObjectProtocol, NSPoint,
    NSRect, NSSize, NSString,
};
use objc2_pdf_kit::{
    PDFAnnotation, PDFAnnotationSubtypeHighlight, PDFView, PDFViewSelectionChangedNotification,
};

#[derive(Debug, Default)]
pub struct FoliumPDFViewIvars {
    active_annotation: RefCell<Option<Retained<PDFAnnotation>>>,
    // Tooltip
    tooltip_tracking: OnceCell<Retained<NSTrackingArea>>,
    current_tooltip: RefCell<Option<Retained<NSString>>>,
    tooltip_panel: OnceCell<Retained<NSPanel>>,
    tooltip_label: OnceCell<Retained<NSTextField>>,
    // Selection action panel
    action_panel: OnceCell<Retained<NSPanel>>,
    action_btn: OnceCell<Retained<NSButton>>,
    selection_observer_registered: OnceCell<()>,
}

define_class!(
    #[unsafe(super = PDFView)]
    #[thread_kind = MainThreadOnly]
    #[ivars = FoliumPDFViewIvars]
    #[name = "FoliumPDFView"]
    #[derive(Debug)]
    pub struct FoliumPDFView;

    unsafe impl NSObjectProtocol for FoliumPDFView {}

    impl FoliumPDFView {
        // ── Context menu ─────────────────────────────────────────

        #[unsafe(method_id(menuForEvent:))]
        fn menu_for_event(&self, event: &NSEvent) -> Option<Retained<NSMenu>> {
            let win_point = event.locationInWindow();
            let view_point = self.convertPoint_fromView(win_point, None);
            let Some(page) = (unsafe { self.pageForPoint_nearest(view_point, false) }) else {
                return unsafe { msg_send![super(self), menuForEvent: event] };
            };
            let page_point = unsafe { self.convertPoint_toPage(view_point, &page) };
            let Some(annotation) = (unsafe { page.annotationAtPoint(page_point) }) else {
                return unsafe { msg_send![super(self), menuForEvent: event] };
            };
            *self.ivars().active_annotation.borrow_mut() = Some(annotation);
            let mtm = MainThreadMarker::from(self);
            Some(self.build_annotation_menu(mtm))
        }

        #[unsafe(method(deleteAnnotation:))]
        fn delete_annotation(&self, _sender: Option<&AnyObject>) {
            let ann = self.ivars().active_annotation.borrow();
            let Some(ref a) = *ann else { return };
            if let Some(page) = unsafe { a.page() } {
                unsafe { page.removeAnnotation(a) };
            }
        }

        #[unsafe(method(addAnnotationNote:))]
        fn add_annotation_note(&self, _sender: Option<&AnyObject>) {
            let annotation = {
                let ann = self.ivars().active_annotation.borrow();
                match *ann {
                    Some(ref a) => a.clone(),
                    None => return,
                }
            };
            self.show_note_dialog(&annotation);
        }

        #[unsafe(method(setAnnotationColorYellow:))]
        fn set_annotation_color_yellow(&self, _sender: Option<&AnyObject>) {
            self.set_active_annotation_color(&NSColor::yellowColor());
        }

        #[unsafe(method(setAnnotationColorGreen:))]
        fn set_annotation_color_green(&self, _sender: Option<&AnyObject>) {
            self.set_active_annotation_color(&NSColor::greenColor());
        }

        #[unsafe(method(setAnnotationColorBlue:))]
        fn set_annotation_color_blue(&self, _sender: Option<&AnyObject>) {
            self.set_active_annotation_color(&NSColor::blueColor());
        }

        #[unsafe(method(setAnnotationColorPink:))]
        fn set_annotation_color_pink(&self, _sender: Option<&AnyObject>) {
            self.set_active_annotation_color(&NSColor::systemPinkColor());
        }

        #[unsafe(method(setAnnotationColorRed:))]
        fn set_annotation_color_red(&self, _sender: Option<&AnyObject>) {
            self.set_active_annotation_color(&NSColor::redColor());
        }

        // ── Highlight action ─────────────────────────────────────

        #[unsafe(method(highlightSelection:))]
        fn highlight_selection(&self, _sender: Option<&AnyObject>) {
            let selection = unsafe { self.currentSelection() };
            let Some(selection) = selection else { return };

            let color = NSColor::yellowColor();
            let pages = unsafe { selection.pages() };
            for i in 0..pages.count() {
                let page = pages.objectAtIndex(i);
                let bounds = unsafe { selection.boundsForPage(&page) };
                let annotation = unsafe {
                    PDFAnnotation::initWithBounds_forType_withProperties(
                        PDFAnnotation::alloc(),
                        bounds,
                        &PDFAnnotationSubtypeHighlight,
                        None,
                    )
                };
                unsafe { annotation.setColor(&color) };
                unsafe { page.addAnnotation(&annotation) };
            }
            unsafe { self.clearSelection() };
        }

        // ── Selection change notification ────────────────────────

        #[unsafe(method(selectionDidChange:))]
        fn selection_did_change(&self, _notification: &NSNotification) {
            let selection = unsafe { self.currentSelection() };
            match selection {
                Some(sel) => {
                    let pages = unsafe { sel.pages() };
                    if pages.count() == 0 {
                        self.hide_action_panel();
                        return;
                    }
                    // Use the first page's selection bounds to position the panel.
                    let page = pages.objectAtIndex(0);
                    let page_rect = unsafe { sel.boundsForPage(&page) };
                    if page_rect.size.width < 1.0 {
                        self.hide_action_panel();
                        return;
                    }
                    let view_rect = unsafe { self.convertRect_fromPage(page_rect, &page) };
                    self.show_action_panel(view_rect);
                }
                None => self.hide_action_panel(),
            }
        }

        // ── Tooltip tracking ─────────────────────────────────────

        #[unsafe(method(updateTrackingAreas))]
        fn update_tracking_areas(&self) {
            unsafe { msg_send![super(self), updateTrackingAreas] }
            // Register selection observer once.
            if self.ivars().selection_observer_registered.get().is_none() {
                self.register_selection_observer();
                let _ = self.ivars().selection_observer_registered.set(());
            }
            if self.ivars().tooltip_tracking.get().is_some() {
                return;
            }
            let options = NSTrackingAreaOptions::MouseEnteredAndExited
                | NSTrackingAreaOptions::MouseMoved
                | NSTrackingAreaOptions::ActiveInKeyWindow
                | NSTrackingAreaOptions::InVisibleRect;
            let owner = self as *const FoliumPDFView as *const AnyObject;
            let tracking_area = unsafe {
                NSTrackingArea::initWithRect_options_owner_userInfo(
                    NSTrackingArea::alloc(),
                    NSRect::ZERO,
                    options,
                    Some(&*owner),
                    None,
                )
            };
            self.addTrackingArea(&tracking_area);
            let _ = self.ivars().tooltip_tracking.set(tracking_area);
        }

        #[unsafe(method(mouseMoved:))]
        fn mouse_moved(&self, event: &NSEvent) {
            let win_point = event.locationInWindow();
            let view_point = self.convertPoint_fromView(win_point, None);

            let new_tooltip: Option<Retained<NSString>> = (|| {
                let page = unsafe { self.pageForPoint_nearest(view_point, false) }?;
                let page_point = unsafe { self.convertPoint_toPage(view_point, &page) };
                let annotation = unsafe { page.annotationAtPoint(page_point) }?;
                let contents = unsafe { annotation.contents() }?;
                if contents.length() > 0 { Some(contents) } else { None }
            })();

            let current = self.ivars().current_tooltip.borrow();
            let changed = match (&*current, &new_tooltip) {
                (None, None) => false,
                (Some(a), Some(b)) => a != b,
                _ => true,
            };
            drop(current);

            if changed {
                match &new_tooltip {
                    Some(text) => self.show_tooltip(text, event),
                    None => self.hide_tooltip(),
                }
                *self.ivars().current_tooltip.borrow_mut() = new_tooltip;
            }

            unsafe { msg_send![super(self), mouseMoved: event] }
        }

        #[unsafe(method(mouseExited:))]
        fn mouse_exited(&self, event: &NSEvent) {
            self.hide_tooltip();
            *self.ivars().current_tooltip.borrow_mut() = None;
            unsafe { msg_send![super(self), mouseExited: event] }
        }
    }
);

impl FoliumPDFView {
    pub fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(FoliumPDFViewIvars::default());
        unsafe { objc2::msg_send![super(this), init] }
    }

    // ── Selection observer ───────────────────────────────────────

    fn register_selection_observer(&self) {
        let center = NSNotificationCenter::defaultCenter();
        let observer = self as *const FoliumPDFView as *const AnyObject;
        let object = self as *const FoliumPDFView as *const AnyObject;
        unsafe {
            center.addObserver_selector_name_object(
                &*observer,
                sel!(selectionDidChange:),
                Some(&PDFViewSelectionChangedNotification),
                Some(&*object),
            );
        }
    }

    // ── Selection action panel ───────────────────────────────────

    fn ensure_action_panel(&self) {
        if self.ivars().action_panel.get().is_some() {
            return;
        }
        let mtm = MainThreadMarker::from(self);

        let target = self as *const FoliumPDFView as *const AnyObject;
        let btn = unsafe {
            NSButton::buttonWithTitle_target_action(
                ns_string!(""),
                Some(&*target),
                Some(sel!(highlightSelection:)),
                mtm,
            )
        };
        if let Some(img) = NSImage::imageWithSystemSymbolName_accessibilityDescription(
            ns_string!("highlighter"),
            Some(ns_string!("Highlight")),
        ) {
            btn.setImage(Some(&img));
        }
        btn.setBezelStyle(NSBezelStyle::Glass);

        let panel: Retained<NSPanel> = unsafe {
            msg_send![
                NSPanel::alloc(mtm),
                initWithContentRect: NSRect::ZERO,
                styleMask: NSWindowStyleMask::empty(),
                backing: NSBackingStoreType::Buffered,
                defer: true
            ]
        };
        panel.setLevel(3); // NSFloatingWindowLevel
        panel.setHasShadow(true);
        unsafe { panel.setReleasedWhenClosed(false) };
        panel.setContentView(Some(&btn));

        let _ = self.ivars().action_btn.set(btn);
        let _ = self.ivars().action_panel.set(panel);
    }

    fn show_action_panel(&self, selection_view_rect: NSRect) {
        self.ensure_action_panel();
        let panel = self.ivars().action_panel.get().unwrap();

        let btn_size = 36.0;
        let Some(window) = self.window() else { return };

        // Position the panel centered above the selection.
        let top_center = NSPoint::new(
            selection_view_rect.origin.x + selection_view_rect.size.width / 2.0,
            selection_view_rect.origin.y + selection_view_rect.size.height,
        );
        let win_point = self.convertPoint_toView(top_center, None);
        let screen_point = window.convertPointToScreen(win_point);
        let origin = NSPoint::new(
            screen_point.x - btn_size / 2.0,
            screen_point.y + 6.0,
        );
        let frame = NSRect::new(origin, NSSize::new(btn_size, btn_size));
        panel.setFrame_display(frame, true);
        panel.orderFront(None);
    }

    fn hide_action_panel(&self) {
        if let Some(panel) = self.ivars().action_panel.get() {
            panel.orderOut(None);
        }
    }

    // ── Tooltip panel ────────────────────────────────────────────

    fn ensure_tooltip_views(&self) {
        if self.ivars().tooltip_panel.get().is_some() {
            return;
        }
        let mtm = MainThreadMarker::from(self);

        let label = NSTextField::new(mtm);
        label.setEditable(false);
        label.setSelectable(false);
        label.setBezeled(false);
        label.setDrawsBackground(true);

        let panel: Retained<NSPanel> = unsafe {
            msg_send![
                NSPanel::alloc(mtm),
                initWithContentRect: NSRect::ZERO,
                styleMask: NSWindowStyleMask::empty(),
                backing: NSBackingStoreType::Buffered,
                defer: true
            ]
        };
        panel.setLevel(3);
        panel.setIgnoresMouseEvents(true);
        panel.setHasShadow(true);
        unsafe { panel.setReleasedWhenClosed(false) };
        panel.setContentView(Some(&label));

        let _ = self.ivars().tooltip_label.set(label);
        let _ = self.ivars().tooltip_panel.set(panel);
    }

    fn show_tooltip(&self, text: &NSString, event: &NSEvent) {
        self.ensure_tooltip_views();
        let panel = self.ivars().tooltip_panel.get().unwrap();
        let label = self.ivars().tooltip_label.get().unwrap();

        label.setStringValue(text);
        label.sizeToFit();
        let label_size = label.frame().size;
        let padding = 6.0;
        let size = NSSize::new(label_size.width + padding * 2.0, label_size.height + padding * 2.0);
        label.setFrame(NSRect::new(NSPoint::new(padding, padding), label_size));

        if let Some(window) = self.window() {
            let screen_point = window.convertPointToScreen(event.locationInWindow());
            let origin = NSPoint::new(screen_point.x + 12.0, screen_point.y - size.height - 4.0);
            panel.setFrame_display(NSRect::new(origin, size), true);
        }
        panel.orderFront(None);
    }

    fn hide_tooltip(&self) {
        if let Some(panel) = self.ivars().tooltip_panel.get() {
            panel.orderOut(None);
        }
    }

    // ── Note dialog ──────────────────────────────────────────────

    fn show_note_dialog(&self, annotation: &PDFAnnotation) {
        let mtm = MainThreadMarker::from(self);
        let has_note = unsafe { annotation.contents() }
            .map(|s| s.length() > 0)
            .unwrap_or(false);

        let alert = NSAlert::new(mtm);
        alert.setMessageText(if has_note {
            ns_string!("Edit Note")
        } else {
            ns_string!("Add Note")
        });
        alert.addButtonWithTitle(ns_string!("Save"));
        alert.addButtonWithTitle(ns_string!("Cancel"));

        let frame = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(300.0, 100.0));
        let scroll_view = NSScrollView::new(mtm);
        scroll_view.setFrame(frame);
        scroll_view.setHasVerticalScroller(true);

        let text_view: Retained<NSTextView> = unsafe {
            objc2::msg_send![NSTextView::alloc(mtm), initWithFrame: frame]
        };
        text_view.setRichText(false);
        if let Some(existing) = unsafe { annotation.contents() } {
            text_view.setString(&existing);
        }

        scroll_view.setDocumentView(Some(&text_view));
        alert.setAccessoryView(Some(&scroll_view));

        let response = alert.runModal();
        if response == 1000 {
            let note = text_view.string();
            if note.length() > 0 {
                unsafe { annotation.setContents(Some(&note)) };
            } else {
                unsafe { annotation.setContents(None) };
            }
        }
    }

    // ── Annotation helpers ───────────────────────────────────────

    fn set_active_annotation_color(&self, color: &NSColor) {
        let ann = self.ivars().active_annotation.borrow();
        let Some(ref a) = *ann else { return };
        unsafe { a.setColor(color) };
    }

    fn build_annotation_menu(&self, mtm: MainThreadMarker) -> Retained<NSMenu> {
        let target = self as *const FoliumPDFView as *const AnyObject;
        let menu = NSMenu::new(mtm);

        let del = make_menu_item(mtm, "Delete Annotation", sel!(deleteAnnotation:), target);
        menu.addItem(&del);

        let has_note = {
            let ann = self.ivars().active_annotation.borrow();
            ann.as_ref()
                .and_then(|a| unsafe { a.contents() })
                .map(|s| s.length() > 0)
                .unwrap_or(false)
        };
        let note_label = if has_note { "Edit Note\u{2026}" } else { "Add Note\u{2026}" };
        let note = make_menu_item(mtm, note_label, sel!(addAnnotationNote:), target);
        menu.addItem(&note);

        menu.addItem(&NSMenuItem::separatorItem(mtm));

        let color_item = unsafe {
            NSMenuItem::initWithTitle_action_keyEquivalent(
                NSMenuItem::alloc(mtm),
                ns_string!("Change Color"),
                None,
                ns_string!(""),
            )
        };
        let color_submenu = NSMenu::initWithTitle(NSMenu::alloc(mtm), ns_string!("Change Color"));
        for (label, action) in [
            ("Yellow", sel!(setAnnotationColorYellow:)),
            ("Green",  sel!(setAnnotationColorGreen:)),
            ("Blue",   sel!(setAnnotationColorBlue:)),
            ("Pink",   sel!(setAnnotationColorPink:)),
            ("Red",    sel!(setAnnotationColorRed:)),
        ] {
            let item = make_menu_item(mtm, label, action, target);
            color_submenu.addItem(&item);
        }
        color_item.setSubmenu(Some(&color_submenu));
        menu.addItem(&color_item);

        menu
    }
}

fn make_menu_item(
    mtm: MainThreadMarker,
    title: &str,
    action: objc2::runtime::Sel,
    target: *const AnyObject,
) -> Retained<NSMenuItem> {
    let item = unsafe {
        NSMenuItem::initWithTitle_action_keyEquivalent(
            NSMenuItem::alloc(mtm),
            &NSString::from_str(title),
            Some(action),
            ns_string!(""),
        )
    };
    unsafe { item.setTarget(Some(&*target)) };
    item
}
