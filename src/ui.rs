use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::sel;
use objc2_app_kit::{
    NSBezelStyle, NSButton, NSColor, NSControlSize, NSImage,
    NSLayoutAttribute, NSLayoutConstraint,
    NSStackView, NSUserInterfaceLayoutOrientation, NSView,
    NSVisualEffectBlendingMode, NSVisualEffectMaterial, NSVisualEffectState,
    NSVisualEffectView,
};
use objc2_foundation::{ns_string, MainThreadMarker, NSArray};
use objc2_pdf_kit::PDFDisplayMode;

use crate::pdf_view::FoliumPDFView;

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
    use objc2_app_kit::NSImageView;
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

/// Build a PDFView configured for folium:
/// continuous single-page layout, page-break shadows (each page appears as
/// a white card floating on the background — PDFKit renders this natively).
pub fn build_pdf_view(mtm: MainThreadMarker) -> Retained<FoliumPDFView> {
    let pdf_view = FoliumPDFView::new(mtm);
    unsafe {
        pdf_view.setDisplayMode(PDFDisplayMode::SinglePageContinuous);
        pdf_view.setDisplaysPageBreaks(true);
        pdf_view.setAutoScales(false);
        pdf_view.setScaleFactor(1.0);
        // Background behind page cards.
        pdf_view.setBackgroundColor(&NSColor::windowBackgroundColor());
    }
    pdf_view
}
