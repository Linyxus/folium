use objc2::rc::Retained;

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

/// Build the PDF content view: a visual effect view containing a scroll view
/// and a frame-based image view as the document view.
pub fn build_pdf_container(
    mtm: MainThreadMarker,
) -> (Retained<NSView>, Retained<NSScrollView>, Retained<NSImageView>) {
    let root_vev = make_visual_effect_view(
        mtm,
        NSVisualEffectMaterial::UnderWindowBackground,
        NSVisualEffectBlendingMode::BehindWindow,
    );
    root_vev.setState(NSVisualEffectState::Active);

    let scroll = NSScrollView::new(mtm);
    scroll.setHasHorizontalScroller(true);
    scroll.setHasVerticalScroller(true);
    scroll.setDrawsBackground(false);
    root_vev.addSubview(&scroll);
    pin_to_superview(&scroll, &root_vev);

    // NSImageScaling(2) == NSImageScaleNone
    let image_view = NSImageView::new(mtm);
    image_view.setImageScaling(NSImageScaling(2));
    scroll.setDocumentView(Some(&*image_view));

    let root = unsafe { Retained::cast_unchecked::<NSView>(root_vev) };
    (root, scroll, image_view)
}
