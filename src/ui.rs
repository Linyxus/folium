use objc2::rc::Retained;
use objc2::{define_class, msg_send, MainThreadOnly};
use objc2_foundation::{NSObjectProtocol, NSPoint};

// NSScrollView subclass that horizontally centers the document when it is
// narrower than the visible area.
//
// `tile` is the designated AppKit hook for scroll-view layout: it is called
// on every resize, magnification change, and content update.  After calling
// super (which lays out scrollers and the clip view), we move the document
// view's origin so it is always centered when the content is narrower than
// the visible area.
define_class!(
    #[unsafe(super = NSScrollView)]
    #[thread_kind = MainThreadOnly]
    #[ivars = ()]
    #[name = "FoliumCenteringScrollView"]
    pub struct CenteringScrollView;

    unsafe impl NSObjectProtocol for CenteringScrollView {}

    impl CenteringScrollView {
        #[unsafe(method(tile))]
        fn tile(&self) {
            unsafe { let _: () = msg_send![super(self), tile]; };

            let Some(doc) = self.documentView() else { return };
            let vis_w  = self.contentSize().width;
            let doc_w  = doc.frame().size.width;
            let new_x  = if doc_w < vis_w {
                ((vis_w - doc_w) / 2.0).floor()
            } else {
                0.0
            };

            // Only move the frame when it actually changed to avoid needless
            // layout passes.
            if (doc.frame().origin.x - new_x).abs() > 0.5 {
                doc.setFrameOrigin(NSPoint { x: new_x, y: doc.frame().origin.y });
            }
        }
    }
);

impl CenteringScrollView {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(());
        unsafe { msg_send![super(this), init] }
    }
}

/// Raw-pointer wrapper that is `Send`.
/// Only dereference on the thread that owns the pointee (main thread for AppKit).
pub struct SendPtr<T>(*const T);
unsafe impl<T> Send for SendPtr<T> {}

impl<T> SendPtr<T> {
    /// Wrap a raw pointer. The caller guarantees the pointee outlives all uses.
    pub fn new(ptr: *const T) -> Self { Self(ptr) }
    /// Recover the raw pointer. Only safe to dereference on the owning thread.
    pub fn as_ptr(&self) -> *const T { self.0 }
}
use objc2::runtime::AnyObject;
use objc2::AnyThread;
use objc2::sel;
use objc2_app_kit::{
    NSBezelStyle, NSBitmapFormat, NSBitmapImageRep, NSButton, NSControlSize,
    NSImage, NSImageScaling, NSImageView, NSLayoutAttribute, NSLayoutConstraint,
    NSScrollView, NSStackView, NSUserInterfaceLayoutOrientation, NSView,
    NSVisualEffectBlendingMode, NSVisualEffectMaterial, NSVisualEffectState,
    NSVisualEffectView,
};
use objc2_foundation::{ns_string, MainThreadMarker, NSArray, NSSize};

pub fn make_visual_effect_view(
    mtm: MainThreadMarker,
    material: NSVisualEffectMaterial,
    blending: NSVisualEffectBlendingMode,
) -> Retained<NSVisualEffectView> {
    let view = NSVisualEffectView::new(mtm);
    view.setMaterial(material);
    view.setBlendingMode(blending);
    view
}

/// Pin all four edges of `view` to `superview` using Auto Layout.
pub fn pin_to_superview(view: &NSView, superview: &NSView) {
    view.setTranslatesAutoresizingMaskIntoConstraints(false);
    NSLayoutConstraint::activateConstraints(&NSArray::from_retained_slice(&[
        view.topAnchor()
            .constraintEqualToAnchor(&superview.topAnchor()),
        view.bottomAnchor()
            .constraintEqualToAnchor(&superview.bottomAnchor()),
        view.leadingAnchor()
            .constraintEqualToAnchor(&superview.leadingAnchor()),
        view.trailingAnchor()
            .constraintEqualToAnchor(&superview.trailingAnchor()),
    ]));
}

/// Convert a raw RGBA buffer into an `NSImage` via `NSBitmapImageRep`.
///
/// `point_size` is the logical (point) dimensions of the image; the pixel
/// buffer must already be rendered at the appropriate backing scale.
/// `NSBitmapImageRep` copies the pixel data during init.
pub fn rgba_to_nsimage(
    rgba: &mut Vec<u8>,
    w: usize,
    h: usize,
    point_size: NSSize,
) -> Retained<NSImage> {
    unsafe {
        let mut plane: *mut u8 = rgba.as_mut_ptr();
        let planes: *mut *mut u8 = &raw mut plane;

        let rep = NSBitmapImageRep::initWithBitmapDataPlanes_pixelsWide_pixelsHigh_bitsPerSample_samplesPerPixel_hasAlpha_isPlanar_colorSpaceName_bitmapFormat_bytesPerRow_bitsPerPixel(
            NSBitmapImageRep::alloc(),
            planes,
            w as isize,
            h as isize,
            8,
            4,
            true,
            false,
            objc2_app_kit::NSDeviceRGBColorSpace,
            NSBitmapFormat(0),
            (w * 4) as isize,
            32,
        )
        .expect("NSBitmapImageRep init failed");

        // Set the rep's size to the point dimensions so NSImage treats it as
        // a HiDPI representation rather than a 1x image.
        rep.setSize(point_size);

        let img = NSImage::initWithSize(NSImage::alloc(), point_size);
        img.addRepresentation(&rep);
        img
    }
}

/// Build the blank "no document" view: a visual effect background with a
/// centered stack containing an SF-Symbol icon and a glass "Open PDF…" button.
pub fn build_blank_view(
    mtm: MainThreadMarker,
    target: &AnyObject,
) -> Retained<NSView> {
    let vev = make_visual_effect_view(
        mtm,
        NSVisualEffectMaterial::UnderWindowBackground,
        NSVisualEffectBlendingMode::BehindWindow,
    );
    vev.setState(NSVisualEffectState::Active);

    // Stack view (vertical, centered)
    let stack = NSStackView::new(mtm);
    stack.setOrientation(NSUserInterfaceLayoutOrientation::Vertical);
    stack.setAlignment(NSLayoutAttribute::CenterX);
    stack.setSpacing(20.0);

    // SF Symbol icon
    let icon_view = NSImageView::new(mtm);
    if let Some(img) = NSImage::imageWithSystemSymbolName_accessibilityDescription(
        ns_string!("doc.fill"),
        None,
    ) {
        icon_view.setImage(Some(&img));
    }
    icon_view.setTranslatesAutoresizingMaskIntoConstraints(false);
    NSLayoutConstraint::activateConstraints(&NSArray::from_retained_slice(&[
        icon_view.widthAnchor().constraintEqualToConstant(64.0),
        icon_view.heightAnchor().constraintEqualToConstant(64.0),
    ]));
    stack.addArrangedSubview(&icon_view);

    // "Open PDF…" button with liquid-glass bezel
    let btn = unsafe {
        NSButton::buttonWithTitle_target_action(
            ns_string!("Open PDF…"),
            Some(target),
            Some(sel!(openDocument:)),
            mtm,
        )
    };
    btn.setBezelStyle(NSBezelStyle::Glass);
    btn.setControlSize(NSControlSize::Large);
    stack.addArrangedSubview(&btn);

    // Constrain stack to center of vev with a slight upward offset
    stack.setTranslatesAutoresizingMaskIntoConstraints(false);
    vev.addSubview(&stack);
    NSLayoutConstraint::activateConstraints(&NSArray::from_retained_slice(&[
        stack.centerXAnchor().constraintEqualToAnchor(&vev.centerXAnchor()),
        stack.centerYAnchor()
            .constraintEqualToAnchor_constant(&vev.centerYAnchor(), -40.0),
    ]));

    // Return as NSView
    unsafe { Retained::cast_unchecked::<NSView>(vev) }
}

/// Padding (in points) added around the PDF page inside the card view.
/// This space lets the drop shadow bleed out from the page without being
/// clipped by the scroll view's clip view.
pub const CARD_PADDING: f64 = 24.0;

/// Build the PDF content view: a visual effect view containing a scroll view
/// whose document is a transparent "card" view.  The PDF image view lives
/// inside the card, inset by `CARD_PADDING` on every side, and carries a
/// drop shadow so the page appears to float above the background.
pub fn build_pdf_container(
    mtm: MainThreadMarker,
) -> (Retained<NSView>, Retained<NSScrollView>, Retained<NSView>, Retained<NSImageView>) {
    let root_vev = make_visual_effect_view(
        mtm,
        NSVisualEffectMaterial::UnderWindowBackground,
        NSVisualEffectBlendingMode::BehindWindow,
    );
    root_vev.setState(NSVisualEffectState::Active);

    let scroll = CenteringScrollView::new(mtm);
    scroll.setHasHorizontalScroller(true);
    scroll.setHasVerticalScroller(true);
    scroll.setDrawsBackground(false);

    root_vev.addSubview(&*scroll);
    pin_to_superview(&scroll, &root_vev);

    // Card view — transparent container that is the scroll view's document.
    // Its extra CARD_PADDING margin gives the drop shadow room to show.
    let card = NSView::new(mtm);

    // NSImageScaling(2) == NSImageScaleNone
    let image_view = NSImageView::new(mtm);
    image_view.setImageScaling(NSImageScaling(2));

    // Layer-back the image view so Core Animation can render a drop shadow.
    image_view.setWantsLayer(true);
    unsafe {
        let layer: *mut AnyObject = msg_send![&*image_view, layer];
        // Shadow colour defaults to black; only opacity and geometry need setting.
        let _: () = msg_send![layer, setShadowOpacity: 0.22_f32];
        let _: () = msg_send![layer, setShadowRadius: 14.0_f64];
        let _: () = msg_send![layer, setShadowOffset: NSSize { width: 0.0, height: -5.0 }];
    }

    card.addSubview(&*image_view);
    scroll.setDocumentView(Some(&*card));

    let root = unsafe { Retained::cast_unchecked::<NSView>(root_vev) };
    let scroll = unsafe { Retained::cast_unchecked::<NSScrollView>(scroll) };
    (root, scroll, card, image_view)
}
