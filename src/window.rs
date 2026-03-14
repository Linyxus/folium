use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{define_class, msg_send, sel, MainThreadOnly};
use objc2_app_kit::{NSBackingStoreType, NSWindow, NSWindowStyleMask};
use objc2_foundation::{MainThreadMarker, NSObjectProtocol, NSRect, NSString};

#[derive(Debug, Default)]
pub struct FoliumWindowIvars {}

define_class!(
    #[unsafe(super = NSWindow)]
    #[thread_kind = MainThreadOnly]
    #[ivars = FoliumWindowIvars]
    #[name = "FoliumWindow"]
    #[derive(Debug)]
    pub struct FoliumWindow;

    unsafe impl NSObjectProtocol for FoliumWindow {}

    impl FoliumWindow {
        /// Intercept the native tab bar being added as a titlebar accessory.
        /// Reposition it to overlap the toolbar so tabs appear *in* the titlebar.
        #[unsafe(method(addTitlebarAccessoryViewController:))]
        fn add_titlebar_accessory_vc(&self, child: &AnyObject) {
            let tab_bar = is_tab_bar(child);
            if tab_bar {
                // NSLayoutAttribute.right = 2 — must be set BEFORE calling super.
                unsafe { let _: () = msg_send![child, setLayoutAttribute: 2_i64]; }
            }
            unsafe { msg_send![super(self), addTitlebarAccessoryViewController: child] }
            if tab_bar {
                // Delay one tick — the private view hierarchy isn't wired until
                // the current run-loop iteration finishes.
                // Then re-apply constraints on ALL windows in the tab group,
                // not just this one — sibling windows also need fixing.
                let null: *const AnyObject = std::ptr::null();
                unsafe {
                    let _: () = msg_send![
                        self,
                        performSelector: sel!(_fixTabBarsInGroup:),
                        withObject: null,
                        afterDelay: 0.0_f64
                    ];
                }
            }
        }

        /// Re-apply tab-in-titlebar constraints on every window in this
        /// window's tab group.
        #[unsafe(method(_fixTabBarsInGroup:))]
        fn _fix_tab_bars_in_group(&self, _sender: Option<&AnyObject>) {
            // Collect all windows in the tab group.
            let windows: Vec<Retained<AnyObject>> = if let Some(tg) = self.tabGroup() {
                let ws: Retained<AnyObject> = unsafe { msg_send![&*tg, windows] };
                let count: usize = unsafe { msg_send![&*ws, count] };
                (0..count)
                    .map(|i| unsafe { msg_send![&*ws, objectAtIndex: i] })
                    .collect()
            } else {
                // No tab group — just fix this window.
                let self_obj: Retained<AnyObject> =
                    unsafe { msg_send![self, self] };
                vec![self_obj]
            };

            for window in &windows {
                fix_tab_bar_on_window(window);
            }
        }
    }
);

impl FoliumWindow {
    pub fn new(
        mtm: MainThreadMarker,
        frame: NSRect,
        style: NSWindowStyleMask,
    ) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(FoliumWindowIvars {});
        unsafe {
            msg_send![
                super(this),
                initWithContentRect: frame,
                styleMask: style,
                backing: NSBackingStoreType::Buffered,
                defer: false
            ]
        }
    }
}

/// The tab bar VC has layoutAttribute == .bottom (4) and no identifier.
fn is_tab_bar(child: &AnyObject) -> bool {
    let attr: i64 = unsafe { msg_send![child, layoutAttribute] };
    if attr != 4 {
        return false;
    }
    let identifier: Option<Retained<AnyObject>> = unsafe { msg_send![child, identifier] };
    identifier.is_none()
}

/// Find the tab bar accessory VC on a window and re-apply constraints.
fn fix_tab_bar_on_window(window: &AnyObject) {
    let vcs: Retained<AnyObject> =
        unsafe { msg_send![window, titlebarAccessoryViewControllers] };
    let count: usize = unsafe { msg_send![&*vcs, count] };
    for i in 0..count {
        let vc: Retained<AnyObject> = unsafe { msg_send![&*vcs, objectAtIndex: i] };
        let attr: i64 = unsafe { msg_send![&*vc, layoutAttribute] };
        // We set the tab bar's layoutAttribute to .right (2).
        // Fresh (not yet processed) tab bars have .bottom (4).
        if attr == 2 || (attr == 4 && {
            let id: Option<Retained<AnyObject>> = unsafe { msg_send![&*vc, identifier] };
            id.is_none()
        }) {
            if attr == 4 {
                unsafe { let _: () = msg_send![&*vc, setLayoutAttribute: 2_i64]; }
            }
            push_tabs_to_titlebar(&vc);
            break;
        }
    }
}

/// Walk the private AppKit view hierarchy and constrain the tab bar's
/// clip view to overlap the NSToolbarView.
///
/// Hierarchy (private):
///   NSTitlebarView
///     ├─ NSToolbarView
///     └─ NSTitlebarAccessoryClipView   ← clip_view
///          └─ accessoryView            ← tab bar
fn push_tabs_to_titlebar(child: &AnyObject) {
    let accessory_view: Retained<AnyObject> = unsafe { msg_send![child, view] };
    let Some(clip_view): Option<Retained<AnyObject>> =
        (unsafe { msg_send![&*accessory_view, superview] })
    else {
        return;
    };
    let Some(titlebar_view): Option<Retained<AnyObject>> =
        (unsafe { msg_send![&*clip_view, superview] })
    else {
        return;
    };

    // Verify we found NSTitlebarView.
    let name: Retained<NSString> = unsafe { msg_send![&*titlebar_view, className] };
    if name.to_string() != "NSTitlebarView" {
        return;
    }

    // Find NSToolbarView among the titlebar's children.
    let subviews: Retained<AnyObject> = unsafe { msg_send![&*titlebar_view, subviews] };
    let count: usize = unsafe { msg_send![&*subviews, count] };
    let mut toolbar_view: Option<Retained<AnyObject>> = None;
    for i in 0..count {
        let sv: Retained<AnyObject> = unsafe { msg_send![&*subviews, objectAtIndex: i] };
        let sv_name: Retained<NSString> = unsafe { msg_send![&*sv, className] };
        if sv_name.to_string() == "NSToolbarView" {
            toolbar_view = Some(sv);
            break;
        }
    }
    let Some(toolbar_view) = toolbar_view else {
        return;
    };

    // Remove any stale constraints from a previous tab bar add/remove cycle.
    unsafe {
        let old_c: Retained<AnyObject> = msg_send![&*clip_view, constraints];
        let _: () = msg_send![&*clip_view, removeConstraints: &*old_c];
        let old_a: Retained<AnyObject> = msg_send![&*accessory_view, constraints];
        let _: () = msg_send![&*accessory_view, removeConstraints: &*old_a];

        let _: () = msg_send![&*clip_view, setTranslatesAutoresizingMaskIntoConstraints: false];
        let _: () =
            msg_send![&*accessory_view, setTranslatesAutoresizingMaskIntoConstraints: false];
    }

    // ── Constrain clip view → toolbar view (with left offset for traffic lights) ──

    let traffic_offset = 78.0_f64;
    apply_constraint_anchor_constant(
        &clip_view,
        c"leftAnchor",
        &toolbar_view,
        c"leftAnchor",
        traffic_offset,
    );
    apply_constraint_anchor(&clip_view, c"rightAnchor", &toolbar_view, c"rightAnchor");
    apply_constraint_anchor(&clip_view, c"topAnchor", &toolbar_view, c"topAnchor");
    apply_constraint_anchor(&clip_view, c"heightAnchor", &toolbar_view, c"heightAnchor");

    // ── Constrain accessory view → clip view (fill) ──

    apply_constraint_anchor(&accessory_view, c"leftAnchor", &clip_view, c"leftAnchor");
    apply_constraint_anchor(&accessory_view, c"rightAnchor", &clip_view, c"rightAnchor");
    apply_constraint_anchor(&accessory_view, c"topAnchor", &clip_view, c"topAnchor");
    apply_constraint_anchor(&accessory_view, c"heightAnchor", &clip_view, c"heightAnchor");
}

fn apply_constraint_anchor(
    view1: &AnyObject,
    anchor1: &std::ffi::CStr,
    view2: &AnyObject,
    anchor2: &std::ffi::CStr,
) {
    let sel1 = objc2::runtime::Sel::register(anchor1);
    let sel2 = objc2::runtime::Sel::register(anchor2);
    unsafe {
        let a1: Retained<AnyObject> = msg_send![view1, performSelector: sel1];
        let a2: Retained<AnyObject> = msg_send![view2, performSelector: sel2];
        let c: Retained<AnyObject> = msg_send![&*a1, constraintEqualToAnchor: &*a2];
        let _: () = msg_send![&*c, setActive: true];
    }
}

fn apply_constraint_anchor_constant(
    view1: &AnyObject,
    anchor1: &std::ffi::CStr,
    view2: &AnyObject,
    anchor2: &std::ffi::CStr,
    constant: f64,
) {
    let sel1 = objc2::runtime::Sel::register(anchor1);
    let sel2 = objc2::runtime::Sel::register(anchor2);
    unsafe {
        let a1: Retained<AnyObject> = msg_send![view1, performSelector: sel1];
        let a2: Retained<AnyObject> = msg_send![view2, performSelector: sel2];
        let c: Retained<AnyObject> =
            msg_send![&*a1, constraintEqualToAnchor: &*a2, constant: constant];
        let _: () = msg_send![&*c, setActive: true];
    }
}
