use std::cell::{OnceCell, RefCell};

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{define_class, msg_send, sel, AnyThread, DefinedClass, MainThreadOnly};
use objc2_app_kit::{
    NSBackingStoreType, NSButton, NSColor, NSEvent, NSFont, NSImage, NSLayoutConstraint, NSPanel,
    NSScrollView, NSTextField, NSTextView, NSTrackingArea, NSTrackingAreaOptions,
    NSVisualEffectBlendingMode, NSVisualEffectMaterial, NSVisualEffectState, NSVisualEffectView,
    NSWindowStyleMask, NSWindowTitleVisibility,
};
use objc2_foundation::{
    ns_string, MainThreadMarker, NSArray, NSNotification, NSNotificationCenter, NSObjectProtocol,
    NSPoint, NSRect, NSSize, NSString,
};
use objc2_pdf_kit::{
    PDFAnnotation, PDFAnnotationSubtypeHighlight, PDFDocument, PDFSelection, PDFView,
    PDFViewPageChangedNotification, PDFViewSelectionChangedNotification,
};

#[derive(Debug, Default)]
pub struct FoliumPDFViewIvars {
    active_annotation: RefCell<Option<Retained<PDFAnnotation>>>,
    // Tooltip
    tooltip_tracking: OnceCell<Retained<NSTrackingArea>>,
    current_tooltip: RefCell<Option<Retained<NSString>>>,
    tooltip_panel: OnceCell<Retained<NSPanel>>,
    tooltip_label: OnceCell<Retained<NSTextField>>,
    // Selection action panel (highlight pill)
    action_panel: OnceCell<Retained<NSPanel>>,
    observers_registered: OnceCell<()>,
    // Page indicator overlay (bottom-right)
    page_indicator: OnceCell<Retained<NSVisualEffectView>>,
    page_indicator_label: OnceCell<Retained<NSTextField>>,
    // Annotation action bar (delete / colors / note)
    annotation_bar: OnceCell<Retained<NSPanel>>,
    // Note editor
    note_panel: OnceCell<Retained<NSPanel>>,
    note_text_view: OnceCell<Retained<NSTextView>>,
    note_annotation: RefCell<Option<Retained<PDFAnnotation>>>,
    // Find bar
    find_panel: OnceCell<Retained<NSPanel>>,
    find_field: OnceCell<Retained<NSTextField>>,
    find_count_label: OnceCell<Retained<NSTextField>>,
    find_query: RefCell<Option<String>>,
    find_document_id: RefCell<Option<usize>>,
    find_matches: RefCell<Vec<Retained<PDFSelection>>>,
    find_match_index: RefCell<usize>,
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
        // ── Mouse handling ───────────────────────────────────────

        #[unsafe(method(mouseDown:))]
        fn mouse_down(&self, event: &NSEvent) {
            self.hide_annotation_bar();
            unsafe { msg_send![super(self), mouseDown: event] }
        }

        #[unsafe(method(mouseUp:))]
        fn mouse_up(&self, event: &NSEvent) {
            unsafe { msg_send![super(self), mouseUp: event] }

            if event.clickCount() != 1 { return; }
            if unsafe { self.currentSelection() }.is_some() { return; }

            let win_point = event.locationInWindow();
            let view_point = self.convertPoint_fromView(win_point, None);
            let Some(page) = (unsafe { self.pageForPoint_nearest(view_point, false) }) else {
                return;
            };
            let page_point = unsafe { self.convertPoint_toPage(view_point, &page) };
            let Some(annotation) = (unsafe { page.annotationAtPoint(page_point) }) else {
                return;
            };

            *self.ivars().active_annotation.borrow_mut() = Some(annotation.clone());
            let page_rect = unsafe { annotation.bounds() };
            let view_rect = unsafe { self.convertRect_fromPage(page_rect, &page) };
            self.show_annotation_bar(view_rect);
        }

        // ── Save ──────────────────────────────────────────────────

        #[unsafe(method(saveDocument:))]
        fn save_document(&self, _sender: Option<&AnyObject>) {
            let doc = unsafe { self.document() };
            let Some(doc) = doc else { return };
            let url = unsafe { doc.documentURL() };
            let Some(url) = url else { return };
            if unsafe { doc.writeToURL(&url) } {
                self.mark_saved();
            }
        }

        // ── Annotation actions ───────────────────────────────────

        #[unsafe(method(deleteAnnotation:))]
        fn delete_annotation(&self, _sender: Option<&AnyObject>) {
            {
                let ann = self.ivars().active_annotation.borrow();
                let Some(ref a) = *ann else { return };
                if let Some(page) = unsafe { a.page() } {
                    unsafe { page.removeAnnotation(a) };
                }
            }
            self.hide_annotation_bar();
            *self.ivars().active_annotation.borrow_mut() = None;
            self.mark_edited();
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
            self.hide_annotation_bar();
            self.show_note_editor(annotation);
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

        // ── Highlight actions ────────────────────────────────────

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
            self.mark_edited();
        }

        #[unsafe(method(highlightAndAddNote:))]
        fn highlight_and_add_note(&self, _sender: Option<&AnyObject>) {
            let selection = unsafe { self.currentSelection() };
            let Some(selection) = selection else { return };

            let color = NSColor::yellowColor();
            let pages = unsafe { selection.pages() };
            let mut first_annotation: Option<Retained<PDFAnnotation>> = None;
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
                if first_annotation.is_none() {
                    first_annotation = Some(annotation);
                }
            }
            unsafe { self.clearSelection() };
            self.mark_edited();

            if let Some(annotation) = first_annotation {
                self.show_note_editor(annotation);
            }
        }

        // ── Note editor actions ──────────────────────────────────

        #[unsafe(method(saveNote:))]
        fn save_note(&self, _sender: Option<&AnyObject>) {
            if let Some(text_view) = self.ivars().note_text_view.get() {
                let note = text_view.string();
                let ann = self.ivars().note_annotation.borrow();
                if let Some(ref a) = *ann {
                    if note.length() > 0 {
                        unsafe { a.setContents(Some(&note)) };
                    } else {
                        unsafe { a.setContents(None) };
                    }
                }
            }
            self.hide_note_editor();
            self.mark_edited();
        }

        #[unsafe(method(cancelNote:))]
        fn cancel_note(&self, _sender: Option<&AnyObject>) {
            self.hide_note_editor();
        }

        // ── Find ──────────────────────────────────────────────────

        #[unsafe(method(showFindBar:))]
        fn show_find_bar(&self, _sender: Option<&AnyObject>) {
            self.ensure_find_panel();
            let panel = self.ivars().find_panel.get().unwrap();
            let field = self.ivars().find_field.get().unwrap();

            // Position at the bottom-center of the PDF view.
            let find_w = 360.0;
            let find_h = 40.0;
            if let Some(window) = self.window() {
                let view_frame = self.frame();
                let bottom_center = objc2_foundation::NSPoint::new(
                    view_frame.origin.x + view_frame.size.width / 2.0,
                    view_frame.origin.y + 8.0,
                );
                let win_point = self.convertPoint_toView(bottom_center, None);
                let screen_point = window.convertPointToScreen(win_point);
                let origin = objc2_foundation::NSPoint::new(
                    screen_point.x - find_w / 2.0,
                    screen_point.y + 4.0,
                );
                panel.setFrame_display(
                    NSRect::new(origin, NSSize::new(find_w, find_h)),
                    true,
                );
            }
            panel.makeKeyAndOrderFront(None);
            panel.makeFirstResponder(Some(field));
        }

        #[unsafe(method(findNext:))]
        fn find_next(&self, _sender: Option<&AnyObject>) {
            self.perform_find(false);
        }

        #[unsafe(method(findPrevious:))]
        fn find_previous(&self, _sender: Option<&AnyObject>) {
            self.perform_find(true);
        }

        #[unsafe(method(dismissFindBar:))]
        fn dismiss_find_bar(&self, _sender: Option<&AnyObject>) {
            self.hide_find_panel();
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

        // ── Page change ──────────────────────────────────────────

        #[unsafe(method(pageDidChange:))]
        fn page_did_change(&self, _notification: &NSNotification) {
            self.update_page_indicator();
        }

        #[unsafe(method(setDocument:))]
        fn set_document_override(&self, doc: Option<&PDFDocument>) {
            unsafe { let _: () = msg_send![super(self), setDocument: doc]; }
            self.update_page_indicator();
        }

        // ── Tooltip tracking ─────────────────────────────────────

        #[unsafe(method(updateTrackingAreas))]
        fn update_tracking_areas(&self) {
            unsafe { msg_send![super(self), updateTrackingAreas] }
            if self.ivars().observers_registered.get().is_none() {
                self.register_observers();
                let _ = self.ivars().observers_registered.set(());
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

    pub fn invalidate_find_results(&self) {
        self.clear_find_results(true);
    }

    // ── Notification observers ───────────────────────────────────

    fn register_observers(&self) {
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
            center.addObserver_selector_name_object(
                &*observer,
                sel!(pageDidChange:),
                Some(&PDFViewPageChangedNotification),
                Some(&*object),
            );
        }
    }

    // ── Page indicator ───────────────────────────────────────────

    fn ensure_page_indicator(&self) {
        if self.ivars().page_indicator.get().is_some() {
            return;
        }
        let mtm = MainThreadMarker::from(self);
        let h = 26.0_f64;

        let vev = NSVisualEffectView::new(mtm);
        vev.setMaterial(NSVisualEffectMaterial::HUDWindow);
        vev.setBlendingMode(NSVisualEffectBlendingMode::WithinWindow);
        vev.setState(NSVisualEffectState::Active);
        vev.setWantsLayer(true);
        unsafe {
            if let Some(layer) = vev.layer() {
                let _: () = msg_send![&*layer, setCornerRadius: h / 2.0];
                let _: () = msg_send![&*layer, setMasksToBounds: true];
            }
        }

        let label = NSTextField::new(mtm);
        label.setEditable(false);
        label.setSelectable(false);
        label.setBezeled(false);
        label.setDrawsBackground(false);
        // Monospaced digits keep "3 / 42" stable as the count rolls over.
        label.setFont(Some(&NSFont::monospacedDigitSystemFontOfSize_weight(11.0, 0.23)));
        label.setTextColor(Some(&NSColor::secondaryLabelColor()));
        label.setStringValue(ns_string!(""));
        label.setTranslatesAutoresizingMaskIntoConstraints(false);

        vev.addSubview(&label);
        vev.setTranslatesAutoresizingMaskIntoConstraints(false);
        vev.setHidden(true);
        self.addSubview(&vev);

        NSLayoutConstraint::activateConstraints(&NSArray::from_retained_slice(&[
            vev.trailingAnchor()
                .constraintEqualToAnchor_constant(&self.trailingAnchor(), -16.0),
            vev.bottomAnchor()
                .constraintEqualToAnchor_constant(&self.bottomAnchor(), -16.0),
            vev.heightAnchor().constraintEqualToConstant(h),
            label.leadingAnchor()
                .constraintEqualToAnchor_constant(&vev.leadingAnchor(), 12.0),
            label.trailingAnchor()
                .constraintEqualToAnchor_constant(&vev.trailingAnchor(), -12.0),
            label.centerYAnchor().constraintEqualToAnchor(&vev.centerYAnchor()),
        ]));

        let _ = self.ivars().page_indicator.set(vev);
        let _ = self.ivars().page_indicator_label.set(label);
    }

    fn update_page_indicator(&self) {
        self.ensure_page_indicator();
        let Some(vev) = self.ivars().page_indicator.get() else { return };
        let Some(label) = self.ivars().page_indicator_label.get() else { return };

        let Some(doc) = (unsafe { self.document() }) else {
            vev.setHidden(true);
            return;
        };
        let total = unsafe { doc.pageCount() };
        if total == 0 {
            vev.setHidden(true);
            return;
        }
        let current_idx = unsafe { self.currentPage() }
            .map(|p| unsafe { doc.indexForPage(&p) })
            .unwrap_or(0);

        let text = NSString::from_str(&format!("{} / {}", current_idx + 1, total));
        label.setStringValue(&text);
        vev.setHidden(false);
    }

    // ── Annotation action bar ────────────────────────────────────

    fn ensure_annotation_bar(&self) {
        if self.ivars().annotation_bar.get().is_some() {
            return;
        }
        let mtm = MainThreadMarker::from(self);
        let target = self as *const FoliumPDFView as *const AnyObject;
        let pill_h: f64 = 36.0;

        // Helper — icon button (delete / note).
        let make_icon_btn =
            |symbol: &NSString, label: &NSString, action: objc2::runtime::Sel| -> Retained<NSButton> {
                let btn = unsafe {
                    NSButton::buttonWithTitle_target_action(
                        ns_string!(""),
                        Some(&*target),
                        Some(action),
                        mtm,
                    )
                };
                if let Some(img) =
                    NSImage::imageWithSystemSymbolName_accessibilityDescription(symbol, Some(label))
                {
                    btn.setImage(Some(&img));
                }
                btn.setBordered(false);
                btn.setTranslatesAutoresizingMaskIntoConstraints(false);
                btn
            };

        // Helper — color dot button.
        let make_color_btn =
            |color: &NSColor, action: objc2::runtime::Sel| -> Retained<NSButton> {
                let btn = unsafe {
                    NSButton::buttonWithTitle_target_action(
                        ns_string!(""),
                        Some(&*target),
                        Some(action),
                        mtm,
                    )
                };
                if let Some(img) = NSImage::imageWithSystemSymbolName_accessibilityDescription(
                    ns_string!("circle.fill"),
                    None,
                ) {
                    btn.setImage(Some(&img));
                }
                unsafe {
                    let _: () = msg_send![&*btn, setContentTintColor: color];
                }
                btn.setBordered(false);
                btn.setTranslatesAutoresizingMaskIntoConstraints(false);
                btn
            };

        let btn_delete = make_icon_btn(
            ns_string!("trash"),
            ns_string!("Delete"),
            sel!(deleteAnnotation:),
        );

        let colors: [(_, objc2::runtime::Sel); 5] = [
            (NSColor::yellowColor(),     sel!(setAnnotationColorYellow:)),
            (NSColor::greenColor(),      sel!(setAnnotationColorGreen:)),
            (NSColor::blueColor(),       sel!(setAnnotationColorBlue:)),
            (NSColor::systemPinkColor(), sel!(setAnnotationColorPink:)),
            (NSColor::redColor(),        sel!(setAnnotationColorRed:)),
        ];
        let color_btns: Vec<Retained<NSButton>> = colors
            .iter()
            .map(|(c, a)| make_color_btn(c, *a))
            .collect();

        let btn_note = make_icon_btn(
            ns_string!("note.text"),
            ns_string!("Add Note"),
            sel!(addAnnotationNote:),
        );

        // Frosted-glass pill.
        let vev = NSVisualEffectView::new(mtm);
        vev.setMaterial(NSVisualEffectMaterial::Popover);
        vev.setBlendingMode(NSVisualEffectBlendingMode::BehindWindow);
        vev.setState(NSVisualEffectState::Active);
        vev.setWantsLayer(true);
        vev.addSubview(&btn_delete);
        for b in &color_btns {
            vev.addSubview(b);
        }
        vev.addSubview(&btn_note);
        unsafe {
            let layer: Option<&AnyObject> = msg_send![&*vev, layer];
            if let Some(layer) = layer {
                let _: () = msg_send![layer, setCornerRadius: pill_h / 2.0];
                let _: () = msg_send![layer, setMasksToBounds: true];
            }
        }

        // Layout — horizontal chain: delete | colors… | note
        let icon_sz = 22.0;
        let dot_sz = 18.0;
        let pad = 10.0;
        let gap = 6.0; // between groups
        let dot_gap = 4.0; // between color dots
        let vev_view: &objc2_app_kit::NSView = &vev;

        // Collect all constraints.
        let mut constraints = vec![
            // Delete — left edge
            btn_delete
                .leadingAnchor()
                .constraintEqualToAnchor_constant(&vev_view.leadingAnchor(), pad),
            btn_delete
                .centerYAnchor()
                .constraintEqualToAnchor(&vev_view.centerYAnchor()),
            btn_delete.widthAnchor().constraintEqualToConstant(icon_sz),
            btn_delete.heightAnchor().constraintEqualToConstant(icon_sz),
        ];

        // Color dots chained after delete.
        let mut prev_trailing = btn_delete.trailingAnchor();
        let mut first_color = true;
        for b in &color_btns {
            let spacing = if first_color { gap } else { dot_gap };
            first_color = false;
            constraints.push(
                b.leadingAnchor()
                    .constraintEqualToAnchor_constant(&prev_trailing, spacing),
            );
            constraints.push(
                b.centerYAnchor()
                    .constraintEqualToAnchor(&vev_view.centerYAnchor()),
            );
            constraints.push(b.widthAnchor().constraintEqualToConstant(dot_sz));
            constraints.push(b.heightAnchor().constraintEqualToConstant(dot_sz));
            prev_trailing = b.trailingAnchor();
        }

        // Note — right edge, chained after last color.
        constraints.push(
            btn_note
                .leadingAnchor()
                .constraintEqualToAnchor_constant(&prev_trailing, gap),
        );
        constraints.push(
            btn_note
                .trailingAnchor()
                .constraintEqualToAnchor_constant(&vev_view.trailingAnchor(), -pad),
        );
        constraints.push(
            btn_note
                .centerYAnchor()
                .constraintEqualToAnchor(&vev_view.centerYAnchor()),
        );
        constraints.push(btn_note.widthAnchor().constraintEqualToConstant(icon_sz));
        constraints.push(btn_note.heightAnchor().constraintEqualToConstant(icon_sz));

        objc2_app_kit::NSLayoutConstraint::activateConstraints(
            &objc2_foundation::NSArray::from_retained_slice(&constraints),
        );

        // Transparent panel.
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
        panel.setHasShadow(true);
        panel.setOpaque(false);
        panel.setBackgroundColor(Some(&NSColor::clearColor()));
        unsafe { panel.setReleasedWhenClosed(false) };
        panel.setContentView(Some(&vev));

        let _ = self.ivars().annotation_bar.set(panel);
    }

    fn show_annotation_bar(&self, annotation_view_rect: NSRect) {
        self.ensure_annotation_bar();
        let panel = self.ivars().annotation_bar.get().unwrap();

        // pill: 10 + 22 + 6 + (18*5 + 4*4) + 6 + 22 + 10 = 182
        let pill_w = 182.0;
        let pill_h = 36.0;
        let Some(window) = self.window() else { return };

        let top_center = NSPoint::new(
            annotation_view_rect.origin.x + annotation_view_rect.size.width / 2.0,
            annotation_view_rect.origin.y + annotation_view_rect.size.height,
        );
        let win_point = self.convertPoint_toView(top_center, None);
        let screen_point = window.convertPointToScreen(win_point);
        let origin = NSPoint::new(
            screen_point.x - pill_w / 2.0,
            screen_point.y + 6.0,
        );
        panel.setFrame_display(NSRect::new(origin, NSSize::new(pill_w, pill_h)), true);
        panel.orderFront(None);
    }

    fn hide_annotation_bar(&self) {
        if let Some(panel) = self.ivars().annotation_bar.get() {
            panel.orderOut(None);
        }
    }

    // ── Selection action panel (highlight pill) ──────────────────

    fn ensure_action_panel(&self) {
        if self.ivars().action_panel.get().is_some() {
            return;
        }
        let mtm = MainThreadMarker::from(self);
        let target = self as *const FoliumPDFView as *const AnyObject;

        let make_icon_btn =
            |symbol: &NSString, label: &NSString, action: objc2::runtime::Sel| -> Retained<NSButton> {
                let btn = unsafe {
                    NSButton::buttonWithTitle_target_action(
                        ns_string!(""),
                        Some(&*target),
                        Some(action),
                        mtm,
                    )
                };
                if let Some(img) =
                    NSImage::imageWithSystemSymbolName_accessibilityDescription(symbol, Some(label))
                {
                    btn.setImage(Some(&img));
                }
                btn.setBordered(false);
                btn.setTranslatesAutoresizingMaskIntoConstraints(false);
                btn
            };

        let btn_highlight = make_icon_btn(
            ns_string!("highlighter"),
            ns_string!("Highlight"),
            sel!(highlightSelection:),
        );
        let btn_note = make_icon_btn(
            ns_string!("note.text"),
            ns_string!("Highlight & Add Note"),
            sel!(highlightAndAddNote:),
        );

        let pill_h: f64 = 36.0;
        let vev = NSVisualEffectView::new(mtm);
        vev.setMaterial(NSVisualEffectMaterial::Popover);
        vev.setBlendingMode(NSVisualEffectBlendingMode::BehindWindow);
        vev.setState(NSVisualEffectState::Active);
        vev.setWantsLayer(true);
        vev.addSubview(&btn_highlight);
        vev.addSubview(&btn_note);
        unsafe {
            let layer: Option<&AnyObject> = msg_send![&*vev, layer];
            if let Some(layer) = layer {
                let _: () = msg_send![layer, setCornerRadius: pill_h / 2.0];
                let _: () = msg_send![layer, setMasksToBounds: true];
            }
        }

        let icon_size = 24.0;
        let h_pad = 8.0;
        let vev_view: &objc2_app_kit::NSView = &vev;
        objc2_app_kit::NSLayoutConstraint::activateConstraints(
            &objc2_foundation::NSArray::from_retained_slice(&[
                btn_highlight
                    .leadingAnchor()
                    .constraintEqualToAnchor_constant(&vev_view.leadingAnchor(), h_pad),
                btn_highlight
                    .centerYAnchor()
                    .constraintEqualToAnchor(&vev_view.centerYAnchor()),
                btn_highlight.widthAnchor().constraintEqualToConstant(icon_size),
                btn_highlight.heightAnchor().constraintEqualToConstant(icon_size),
                btn_note
                    .leadingAnchor()
                    .constraintEqualToAnchor_constant(&btn_highlight.trailingAnchor(), h_pad),
                btn_note
                    .trailingAnchor()
                    .constraintEqualToAnchor_constant(&vev_view.trailingAnchor(), -h_pad),
                btn_note
                    .centerYAnchor()
                    .constraintEqualToAnchor(&vev_view.centerYAnchor()),
                btn_note.widthAnchor().constraintEqualToConstant(icon_size),
                btn_note.heightAnchor().constraintEqualToConstant(icon_size),
            ]),
        );

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
        panel.setHasShadow(true);
        panel.setOpaque(false);
        panel.setBackgroundColor(Some(&NSColor::clearColor()));
        unsafe { panel.setReleasedWhenClosed(false) };
        panel.setContentView(Some(&vev));

        let _ = self.ivars().action_panel.set(panel);
    }

    fn show_action_panel(&self, selection_view_rect: NSRect) {
        self.ensure_action_panel();
        let panel = self.ivars().action_panel.get().unwrap();

        let pill_w = 72.0;
        let pill_h = 36.0;
        let Some(window) = self.window() else { return };

        let top_center = NSPoint::new(
            selection_view_rect.origin.x + selection_view_rect.size.width / 2.0,
            selection_view_rect.origin.y + selection_view_rect.size.height,
        );
        let win_point = self.convertPoint_toView(top_center, None);
        let screen_point = window.convertPointToScreen(win_point);
        let origin = NSPoint::new(
            screen_point.x - pill_w / 2.0,
            screen_point.y + 6.0,
        );
        panel.setFrame_display(NSRect::new(origin, NSSize::new(pill_w, pill_h)), true);
        panel.orderFront(None);
    }

    fn hide_action_panel(&self) {
        if let Some(panel) = self.ivars().action_panel.get() {
            panel.orderOut(None);
        }
    }

    // ── Note editor ──────────────────────────────────────────────

    fn ensure_note_panel(&self) {
        if self.ivars().note_panel.get().is_some() {
            return;
        }
        let mtm = MainThreadMarker::from(self);
        let target = self as *const FoliumPDFView as *const AnyObject;
        let panel_w = 280.0_f64;
        let panel_h = 120.0_f64;

        let tv_frame = NSRect::new(NSPoint::ZERO, NSSize::new(panel_w, panel_h));
        let text_view: Retained<NSTextView> = unsafe {
            objc2::msg_send![NSTextView::alloc(mtm), initWithFrame: tv_frame]
        };
        text_view.setRichText(false);
        text_view.setDrawsBackground(false);
        text_view.setTextColor(Some(&NSColor::labelColor()));
        text_view.setFont(Some(&NSFont::systemFontOfSize(13.0)));
        text_view.setTranslatesAutoresizingMaskIntoConstraints(false);

        let scroll_view = NSScrollView::new(mtm);
        scroll_view.setHasVerticalScroller(true);
        scroll_view.setDrawsBackground(false);
        scroll_view.setTranslatesAutoresizingMaskIntoConstraints(false);
        scroll_view.setDocumentView(Some(&text_view));

        // Cancel button — Escape shortcut
        let cancel_btn = unsafe {
            NSButton::buttonWithTitle_target_action(
                ns_string!("Cancel  Esc"),
                Some(&*target),
                Some(sel!(cancelNote:)),
                mtm,
            )
        };
        cancel_btn.setKeyEquivalent(ns_string!("\u{1b}")); // Escape
        cancel_btn.setTranslatesAutoresizingMaskIntoConstraints(false);

        // Ok button — Cmd+Enter shortcut
        let save_btn = unsafe {
            NSButton::buttonWithTitle_target_action(
                ns_string!("Ok  \u{2318}\u{23ce}"),
                Some(&*target),
                Some(sel!(saveNote:)),
                mtm,
            )
        };
        save_btn.setKeyEquivalent(ns_string!("\r"));
        unsafe {
            let mods = objc2_app_kit::NSEventModifierFlags::Command;
            let _: () = msg_send![&*save_btn, setKeyEquivalentModifierMask: mods];
        }
        save_btn.setTranslatesAutoresizingMaskIntoConstraints(false);

        let vev = NSVisualEffectView::new(mtm);
        vev.setMaterial(NSVisualEffectMaterial::Popover);
        vev.setBlendingMode(NSVisualEffectBlendingMode::BehindWindow);
        vev.setState(NSVisualEffectState::Active);
        vev.setWantsLayer(true);
        vev.addSubview(&scroll_view);
        vev.addSubview(&save_btn);
        vev.addSubview(&cancel_btn);
        unsafe {
            let layer: Option<&AnyObject> = msg_send![&*vev, layer];
            if let Some(layer) = layer {
                let _: () = msg_send![layer, setCornerRadius: 12.0_f64];
                let _: () = msg_send![layer, setMasksToBounds: true];
            }
        }

        let pad = 12.0;
        let vev_view: &objc2_app_kit::NSView = &vev;
        objc2_app_kit::NSLayoutConstraint::activateConstraints(
            &objc2_foundation::NSArray::from_retained_slice(&[
                // Text area
                scroll_view
                    .topAnchor()
                    .constraintEqualToAnchor_constant(&vev_view.topAnchor(), pad),
                scroll_view
                    .leadingAnchor()
                    .constraintEqualToAnchor_constant(&vev_view.leadingAnchor(), pad),
                scroll_view
                    .trailingAnchor()
                    .constraintEqualToAnchor_constant(&vev_view.trailingAnchor(), -pad),
                // Button row
                save_btn
                    .topAnchor()
                    .constraintEqualToAnchor_constant(&scroll_view.bottomAnchor(), 6.0),
                save_btn
                    .bottomAnchor()
                    .constraintEqualToAnchor_constant(&vev_view.bottomAnchor(), -8.0),
                save_btn
                    .trailingAnchor()
                    .constraintEqualToAnchor_constant(&vev_view.trailingAnchor(), -pad),
                cancel_btn
                    .centerYAnchor()
                    .constraintEqualToAnchor(&save_btn.centerYAnchor()),
                cancel_btn
                    .trailingAnchor()
                    .constraintEqualToAnchor_constant(&save_btn.leadingAnchor(), -8.0),
            ]),
        );

        let style = NSWindowStyleMask::Titled | NSWindowStyleMask::FullSizeContentView;
        let panel: Retained<NSPanel> = unsafe {
            msg_send![
                NSPanel::alloc(mtm),
                initWithContentRect: NSRect::new(NSPoint::ZERO, NSSize::new(panel_w, panel_h)),
                styleMask: style,
                backing: NSBackingStoreType::Buffered,
                defer: true
            ]
        };
        panel.setLevel(3);
        panel.setHasShadow(true);
        panel.setOpaque(false);
        panel.setBackgroundColor(Some(&NSColor::clearColor()));
        panel.setTitlebarAppearsTransparent(true);
        panel.setTitleVisibility(NSWindowTitleVisibility::Hidden);
        panel.setMovable(true);
        unsafe { panel.setReleasedWhenClosed(false) };
        panel.setContentView(Some(&vev));

        let _ = self.ivars().note_text_view.set(text_view);
        let _ = self.ivars().note_panel.set(panel);
    }

    fn show_note_editor(&self, annotation: Retained<PDFAnnotation>) {
        self.ensure_note_panel();
        let panel = self.ivars().note_panel.get().unwrap();
        let text_view = self.ivars().note_text_view.get().unwrap();

        if let Some(existing) = unsafe { annotation.contents() } {
            text_view.setString(&existing);
        } else {
            text_view.setString(ns_string!(""));
        }

        let panel_w = 280.0;
        let panel_h = 120.0;
        if let Some(page) = unsafe { annotation.page() } {
            let page_rect = unsafe { annotation.bounds() };
            let view_rect = unsafe { self.convertRect_fromPage(page_rect, &page) };
            if let Some(window) = self.window() {
                let top_left = NSPoint::new(
                    view_rect.origin.x,
                    view_rect.origin.y + view_rect.size.height,
                );
                let win_point = self.convertPoint_toView(top_left, None);
                let screen_point = window.convertPointToScreen(win_point);
                let origin = NSPoint::new(screen_point.x, screen_point.y + 6.0);
                panel.setFrame_display(
                    NSRect::new(origin, NSSize::new(panel_w, panel_h)),
                    true,
                );
            }
        }

        *self.ivars().note_annotation.borrow_mut() = Some(annotation);
        panel.makeKeyAndOrderFront(None);
        panel.makeFirstResponder(Some(text_view));
    }

    fn hide_note_editor(&self) {
        if let Some(panel) = self.ivars().note_panel.get() {
            panel.orderOut(None);
        }
        *self.ivars().note_annotation.borrow_mut() = None;
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
        label.setDrawsBackground(false);
        label.setTextColor(Some(&NSColor::labelColor()));
        label.setFont(Some(&NSFont::systemFontOfSize(NSFont::smallSystemFontSize())));

        let vev = NSVisualEffectView::new(mtm);
        vev.setMaterial(NSVisualEffectMaterial::Popover);
        vev.setBlendingMode(NSVisualEffectBlendingMode::BehindWindow);
        vev.setState(NSVisualEffectState::Active);
        vev.addSubview(&label);

        vev.setWantsLayer(true);
        unsafe {
            let layer: Option<&AnyObject> = msg_send![&*vev, layer];
            if let Some(layer) = layer {
                let _: () = msg_send![layer, setCornerRadius: 8.0_f64];
                let _: () = msg_send![layer, setMasksToBounds: true];
            }
        }

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
        panel.setOpaque(false);
        panel.setBackgroundColor(Some(&NSColor::clearColor()));
        unsafe { panel.setReleasedWhenClosed(false) };
        panel.setContentView(Some(&vev));

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
        let h_pad = 12.0;
        let v_pad = 8.0;
        let size = NSSize::new(label_size.width + h_pad * 2.0, label_size.height + v_pad * 2.0);
        label.setFrame(NSRect::new(NSPoint::new(h_pad, v_pad), label_size));

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

    // ── Find panel ───────────────────────────────────────────────

    fn ensure_find_panel(&self) {
        if self.ivars().find_panel.get().is_some() {
            return;
        }
        let mtm = MainThreadMarker::from(self);
        let target = self as *const FoliumPDFView as *const AnyObject;

        // Search field — Enter triggers findNext:.
        let field = NSTextField::new(mtm);
        field.setPlaceholderString(Some(ns_string!("Search in PDF\u{2026}")));
        field.setFont(Some(&NSFont::systemFontOfSize(13.0)));
        field.setDrawsBackground(false);
        field.setBezeled(false);
        field.setFocusRingType(objc2_app_kit::NSFocusRingType::None);
        field.setTranslatesAutoresizingMaskIntoConstraints(false);
        unsafe {
            let _: () = msg_send![&*field, setTarget: &*target];
            let _: () = msg_send![&*field, setAction: sel!(findNext:)];
        }

        // Navigation buttons.
        let make_btn = |symbol: &NSString, label: &NSString, action: objc2::runtime::Sel| -> Retained<NSButton> {
            let btn = unsafe {
                NSButton::buttonWithTitle_target_action(
                    ns_string!(""),
                    Some(&*target),
                    Some(action),
                    mtm,
                )
            };
            if let Some(img) =
                NSImage::imageWithSystemSymbolName_accessibilityDescription(symbol, Some(label))
            {
                btn.setImage(Some(&img));
            }
            btn.setBordered(false);
            btn.setTranslatesAutoresizingMaskIntoConstraints(false);
            btn
        };

        let btn_prev = make_btn(
            ns_string!("chevron.up"),
            ns_string!("Previous"),
            sel!(findPrevious:),
        );
        let btn_next = make_btn(
            ns_string!("chevron.down"),
            ns_string!("Next"),
            sel!(findNext:),
        );
        let btn_close = make_btn(
            ns_string!("xmark"),
            ns_string!("Close"),
            sel!(dismissFindBar:),
        );
        btn_close.setKeyEquivalent(ns_string!("\u{1b}")); // Escape

        // Match count label.
        let count_label = NSTextField::new(mtm);
        count_label.setEditable(false);
        count_label.setSelectable(false);
        count_label.setBezeled(false);
        count_label.setDrawsBackground(false);
        count_label.setStringValue(ns_string!(""));
        count_label.setFont(Some(&NSFont::systemFontOfSize(11.0)));
        count_label.setTextColor(Some(&NSColor::secondaryLabelColor()));
        count_label.setTranslatesAutoresizingMaskIntoConstraints(false);

        // Glass backdrop.
        let vev = NSVisualEffectView::new(mtm);
        vev.setMaterial(NSVisualEffectMaterial::Popover);
        vev.setBlendingMode(NSVisualEffectBlendingMode::BehindWindow);
        vev.setState(NSVisualEffectState::Active);
        vev.setWantsLayer(true);
        vev.addSubview(&field);
        vev.addSubview(&count_label);
        vev.addSubview(&btn_prev);
        vev.addSubview(&btn_next);
        vev.addSubview(&btn_close);
        unsafe {
            let layer: Option<&AnyObject> = msg_send![&*vev, layer];
            if let Some(layer) = layer {
                let _: () = msg_send![layer, setCornerRadius: 10.0_f64];
                let _: () = msg_send![layer, setMasksToBounds: true];
            }
        }

        // Layout: [ field ──────────  N/M ] [▲] [▼] [✕]
        let icon_sz = 18.0;
        let pad = 10.0;
        let vev_view: &objc2_app_kit::NSView = &vev;
        objc2_app_kit::NSLayoutConstraint::activateConstraints(
            &objc2_foundation::NSArray::from_retained_slice(&[
                field
                    .leadingAnchor()
                    .constraintEqualToAnchor_constant(&vev_view.leadingAnchor(), pad),
                field
                    .centerYAnchor()
                    .constraintEqualToAnchor(&vev_view.centerYAnchor()),
                field
                    .trailingAnchor()
                    .constraintEqualToAnchor_constant(&count_label.leadingAnchor(), -4.0),
                count_label
                    .centerYAnchor()
                    .constraintEqualToAnchor(&vev_view.centerYAnchor()),
                count_label
                    .trailingAnchor()
                    .constraintEqualToAnchor_constant(&btn_prev.leadingAnchor(), -6.0),
                btn_prev
                    .centerYAnchor()
                    .constraintEqualToAnchor(&vev_view.centerYAnchor()),
                btn_prev.widthAnchor().constraintEqualToConstant(icon_sz),
                btn_prev.heightAnchor().constraintEqualToConstant(icon_sz),
                btn_next
                    .leadingAnchor()
                    .constraintEqualToAnchor_constant(&btn_prev.trailingAnchor(), 2.0),
                btn_next
                    .centerYAnchor()
                    .constraintEqualToAnchor(&vev_view.centerYAnchor()),
                btn_next.widthAnchor().constraintEqualToConstant(icon_sz),
                btn_next.heightAnchor().constraintEqualToConstant(icon_sz),
                btn_close
                    .leadingAnchor()
                    .constraintEqualToAnchor_constant(&btn_next.trailingAnchor(), 6.0),
                btn_close
                    .trailingAnchor()
                    .constraintEqualToAnchor_constant(&vev_view.trailingAnchor(), -pad),
                btn_close
                    .centerYAnchor()
                    .constraintEqualToAnchor(&vev_view.centerYAnchor()),
                btn_close.widthAnchor().constraintEqualToConstant(icon_sz),
                btn_close.heightAnchor().constraintEqualToConstant(icon_sz),
            ]),
        );

        // Panel — Titled + FullSizeContentView so it can become key for text input.
        let style = NSWindowStyleMask::Titled | NSWindowStyleMask::FullSizeContentView;
        let panel: Retained<NSPanel> = unsafe {
            msg_send![
                NSPanel::alloc(mtm),
                initWithContentRect: NSRect::ZERO,
                styleMask: style,
                backing: NSBackingStoreType::Buffered,
                defer: true
            ]
        };
        panel.setLevel(3);
        panel.setHasShadow(true);
        panel.setOpaque(false);
        panel.setBackgroundColor(Some(&NSColor::clearColor()));
        panel.setTitlebarAppearsTransparent(true);
        panel.setTitleVisibility(NSWindowTitleVisibility::Hidden);
        unsafe { panel.setReleasedWhenClosed(false) };
        panel.setContentView(Some(&vev));

        let _ = self.ivars().find_field.set(field);
        let _ = self.ivars().find_count_label.set(count_label);
        let _ = self.ivars().find_panel.set(panel);
    }

    fn perform_find(&self, backwards: bool) {
        let Some(field) = self.ivars().find_field.get() else { return };
        let query = field.stringValue();
        if query.length() == 0 {
            self.clear_find_results(true);
            return;
        }
        let query = query.to_string();
        let recomputed = match self.refresh_find_matches(&query) {
            Some(recomputed) => recomputed,
            None => return,
        };

        let total = self.ivars().find_matches.borrow().len();
        if total == 0 {
            self.update_find_count_label(0, 0);
            unsafe { self.setCurrentSelection_animate(None, false) };
            return;
        }

        // Determine the next index based on direction.
        let idx = {
            let current_idx = *self.ivars().find_match_index.borrow();
            if recomputed {
                if backwards { total - 1 } else { 0 }
            } else if backwards {
                if current_idx == 0 { total - 1 } else { current_idx - 1 }
            } else if current_idx + 1 >= total {
                0
            } else {
                current_idx + 1
            }
        };

        *self.ivars().find_match_index.borrow_mut() = idx;

        let matches = self.ivars().find_matches.borrow();
        let sel = &matches[idx];
        unsafe {
            self.setCurrentSelection_animate(Some(sel), true);
            self.scrollSelectionToVisible(None);
        }
        self.update_find_count_label(idx + 1, total);
    }

    fn refresh_find_matches(&self, query: &str) -> Option<bool> {
        let doc = unsafe { self.document() };
        let Some(doc) = doc else {
            self.clear_find_results(true);
            return None;
        };

        let document_id = Retained::as_ptr(&doc) as usize;
        let query_changed = self.ivars().find_query.borrow().as_deref() != Some(query);
        let document_changed = *self.ivars().find_document_id.borrow() != Some(document_id);
        if !query_changed && !document_changed {
            return Some(false);
        }

        // Collect all matches once per (document, query) pair.
        let query_ns = NSString::from_str(query);
        let options = objc2_foundation::NSStringCompareOptions(1); // NSCaseInsensitiveSearch
        let all_matches = unsafe { doc.findString_withOptions(&query_ns, options) };
        let matches: Vec<Retained<PDFSelection>> = (0..all_matches.count())
            .map(|i| all_matches.objectAtIndex(i))
            .collect();

        *self.ivars().find_query.borrow_mut() = Some(query.to_owned());
        *self.ivars().find_document_id.borrow_mut() = Some(document_id);
        *self.ivars().find_matches.borrow_mut() = matches;
        *self.ivars().find_match_index.borrow_mut() = 0;

        Some(true)
    }

    fn clear_find_results(&self, clear_selection: bool) {
        *self.ivars().find_query.borrow_mut() = None;
        *self.ivars().find_document_id.borrow_mut() = None;
        self.ivars().find_matches.borrow_mut().clear();
        *self.ivars().find_match_index.borrow_mut() = 0;
        self.update_find_count_label(0, 0);

        if clear_selection {
            unsafe { self.setCurrentSelection_animate(None, false) };
        }
    }

    fn update_find_count_label(&self, current: usize, total: usize) {
        let Some(label) = self.ivars().find_count_label.get() else { return };
        if total == 0 {
            label.setStringValue(ns_string!(""));
        } else {
            let text = NSString::from_str(&format!("{}/{}", current, total));
            label.setStringValue(&text);
        }
        label.sizeToFit();
    }

    fn hide_find_panel(&self) {
        if let Some(panel) = self.ivars().find_panel.get() {
            panel.orderOut(None);
        }
        self.clear_find_results(true);
    }

    // ── Annotation helpers ───────────────────────────────────────

    fn set_active_annotation_color(&self, color: &NSColor) {
        let ann = self.ivars().active_annotation.borrow();
        let Some(ref a) = *ann else { return };
        unsafe { a.setColor(color) };
        drop(ann);
        self.mark_edited();
    }

    fn mark_edited(&self) {
        if let Some(window) = self.window() {
            window.setDocumentEdited(true);
        }
    }

    fn mark_saved(&self) {
        if let Some(window) = self.window() {
            window.setDocumentEdited(false);
        }
    }
}
