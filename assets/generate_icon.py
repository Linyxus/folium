"""Generate the Folium app icon — an elegant single leaf on warm parchment."""

from PIL import Image, ImageDraw
import math, os, subprocess

SIZE = 1024
CX, CY = SIZE // 2, SIZE // 2 + 30
OUT_DIR = os.path.dirname(os.path.abspath(__file__))

# ── Colours ───────────────────────────────────────────────────────
BG       = (250, 245, 235, 255)   # warm parchment cream
LEAF     = ( 76, 145,  85, 255)   # rich sage green
LEAF_DK  = ( 55, 110,  62, 255)   # darker green for shadow leaf
VEIN     = (245, 240, 232, 180)   # subtle light vein
STEM     = ( 62, 120,  70, 255)   # stem colour


def bezier(p0, p1, p2, p3, steps=80):
    """Cubic Bezier curve from p0 to p3 with control points p1, p2."""
    pts = []
    for i in range(steps + 1):
        t = i / steps
        u = 1 - t
        x = u**3*p0[0] + 3*u**2*t*p1[0] + 3*u*t**2*p2[0] + t**3*p3[0]
        y = u**3*p0[1] + 3*u**2*t*p1[1] + 3*u*t**2*p2[1] + t**3*p3[1]
        pts.append((x, y))
    return pts


def leaf_outline(cx, cy, h, w):
    """Build a leaf polygon: pointed at top, rounded at bottom, with a gentle taper."""
    top = (cx, cy - h * 0.48)       # tip
    bot = (cx, cy + h * 0.48)       # base (stem junction)
    mid_w = w * 0.50                # max half-width

    # Right side: top → bottom (two Bezier segments for natural curvature)
    # Upper right: tip → widest point
    right_mid = (cx + mid_w, cy - h * 0.05)
    right_upper = bezier(
        top,
        (cx + mid_w * 0.35, cy - h * 0.40),   # control 1 — gentle start
        (cx + mid_w * 1.05, cy - h * 0.22),   # control 2 — swells outward
        right_mid,
    )
    # Lower right: widest point → base
    right_lower = bezier(
        right_mid,
        (cx + mid_w * 1.0,  cy + h * 0.18),   # control 1 — still wide
        (cx + mid_w * 0.30, cy + h * 0.42),   # control 2 — tapers inward
        bot,
    )

    # Left side: bottom → top (mirror)
    left_mid = (cx - mid_w, cy - h * 0.05)
    left_lower = bezier(
        bot,
        (cx - mid_w * 0.30, cy + h * 0.42),
        (cx - mid_w * 1.0,  cy + h * 0.18),
        left_mid,
    )
    left_upper = bezier(
        left_mid,
        (cx - mid_w * 1.05, cy - h * 0.22),
        (cx - mid_w * 0.35, cy - h * 0.40),
        top,
    )

    # Combine into one polygon (clockwise)
    outline = right_upper + right_lower[1:] + left_lower[1:] + left_upper[1:]
    return outline


def draw_vein(draw, x0, y0, x1, y1, width, color):
    """A single straight vein with taper via overlapping lines."""
    steps = 20
    for i in range(steps):
        t = i / steps
        ax = x0 + (x1 - x0) * t
        ay = y0 + (y1 - y0) * t
        bx = x0 + (x1 - x0) * (t + 1/steps)
        by = y0 + (y1 - y0) * (t + 1/steps)
        w = max(1, int(width * (1 - t * 0.8)))
        draw.line([(ax, ay), (bx, by)], fill=color, width=w)


def draw_veins(draw, cx, cy, h, w):
    """Central midrib + branching side veins."""
    top_y = cy - h * 0.42
    bot_y = cy + h * 0.42
    mid_w = w * 0.50

    # Midrib
    draw_vein(draw, cx, bot_y, cx, top_y, 5, VEIN)

    # Side veins: (fraction along midrib from bottom, length fraction, angle)
    vein_specs = [
        (0.20, 0.55, 0.45),
        (0.35, 0.65, 0.40),
        (0.50, 0.72, 0.38),
        (0.65, 0.65, 0.40),
        (0.78, 0.50, 0.45),
        (0.88, 0.32, 0.50),
    ]
    for frac, length_frac, angle_frac in vein_specs:
        vy = bot_y + (top_y - bot_y) * frac
        vlen = mid_w * length_frac
        angle = math.pi * angle_frac  # angle from horizontal

        # Right vein
        ex = cx + vlen * math.cos(angle)
        ey = vy - vlen * math.sin(angle)
        draw_vein(draw, cx, vy, ex, ey, 3, VEIN)

        # Left vein (mirror)
        ex = cx - vlen * math.cos(angle)
        draw_vein(draw, cx, vy, ex, ey, 3, VEIN)


def generate_icon():
    img = Image.new("RGBA", (SIZE, SIZE), BG)

    leaf_h = 620
    leaf_w = 480

    # ── Shadow layer ──
    shadow = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
    sd = ImageDraw.Draw(shadow)
    shadow_outline = leaf_outline(CX + 5, CY + 8, leaf_h, leaf_w)
    sd.polygon(shadow_outline, fill=(0, 0, 0, 35))
    img = Image.alpha_composite(img, shadow)

    # ── Leaf body ──
    draw = ImageDraw.Draw(img)
    outline = leaf_outline(CX, CY, leaf_h, leaf_w)
    draw.polygon(outline, fill=LEAF)

    # ── Subtle highlight (upper half lighter) ──
    highlight = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
    hd = ImageDraw.Draw(highlight)
    hd.polygon(outline, fill=(255, 255, 255, 0))
    for y in range(SIZE):
        t = y / SIZE
        if t < 0.45:
            alpha = int(22 * (1 - t / 0.45))
            hd.line([(0, y), (SIZE, y)], fill=(255, 255, 255, alpha))
    img = Image.alpha_composite(img, highlight)
    draw = ImageDraw.Draw(img)

    # ── Veins ──
    draw_veins(draw, CX, CY, leaf_h, leaf_w)

    # ── Stem ──
    stem_top = CY + leaf_h * 0.48
    stem_bot = stem_top + 55
    draw.line([(CX, stem_top), (CX + 3, stem_bot)], fill=STEM, width=6)

    # ── Save ──
    src_path = os.path.join(OUT_DIR, "icon_1024.png")
    img.save(src_path, "PNG")
    print(f"  {src_path}")

    # ── Build .iconset → .icns ──
    iconset = os.path.join(OUT_DIR, "AppIcon.iconset")
    os.makedirs(iconset, exist_ok=True)
    sizes = [
        ("icon_16x16.png", 16), ("icon_16x16@2x.png", 32),
        ("icon_32x32.png", 32), ("icon_32x32@2x.png", 64),
        ("icon_128x128.png", 128), ("icon_128x128@2x.png", 256),
        ("icon_256x256.png", 256), ("icon_256x256@2x.png", 512),
        ("icon_512x512.png", 512), ("icon_512x512@2x.png", 1024),
    ]
    for name, px in sizes:
        img.resize((px, px), Image.LANCZOS).save(os.path.join(iconset, name), "PNG")

    icns_path = os.path.join(OUT_DIR, "AppIcon.icns")
    subprocess.run(["iconutil", "-c", "icns", iconset, "-o", icns_path], check=True)
    print(f"  {icns_path}")


if __name__ == "__main__":
    generate_icon()
