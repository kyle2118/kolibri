"""Build the app icons from the transparent logo artwork.

The app icon is the bird on a light rounded square (white to soft lavender,
faint rim, soft shadow), laid out on the macOS icon grid. The sidebar logo
is the bare bird.

usage (needs Pillow):  python3 scripts/make-icons.py assets/new-logo.png assets
then:                  iconutil -c icns assets/kolibri.iconset -o assets/kolibri.icns
                       rm -r assets/kolibri.iconset
"""
import os
import sys

from PIL import Image, ImageChops, ImageDraw, ImageFilter

src, out = sys.argv[1], sys.argv[2]
logo = Image.open(src).convert("RGBA")
# Crop to the visible bird (ignore near-transparent fringe).
mask = logo.getchannel("A").point(lambda v: 255 if v > 40 else 0)
logo = logo.crop(mask.getbbox())

# macOS icon grid on a 1024 canvas: 824 tile, ~22.4% corner radius.
CANVAS, TILE = 1024, 824
RADIUS = round(TILE * 0.2237)
TOP_LEFT = (253, 253, 254)  # tile gradient, from the sample
BOTTOM_RIGHT = (232, 234, 249)
BIRD_WIDTH = 0.81  # of the tile width
BIRD_SHIFT_X = 0.033  # right of center, as in the sample (balances the long beak)
SS = 4  # supersampling for smooth edges


def scaled_logo(width):
    w, h = logo.size
    return logo.resize((round(width), round(h * width / w)), Image.LANCZOS)


def rounded_mask(size, box, radius):
    """Anti-aliased rounded-rectangle mask."""
    big = Image.new("L", (size * SS, size * SS), 0)
    x0, y0, x1, y1 = (v * SS for v in box)
    ImageDraw.Draw(big).rounded_rectangle((x0, y0, x1 - 1, y1 - 1), radius * SS, fill=255)
    return big.resize((size, size), Image.LANCZOS)


def app_icon():
    off = (CANVAS - TILE) // 2
    box = (off, off, off + TILE, off + TILE)
    icon = Image.new("RGBA", (CANVAS, CANVAS), (0, 0, 0, 0))

    # Soft shadow below the tile.
    shadow_mask = rounded_mask(CANVAS, (box[0], box[1] + 12, box[2], box[3] + 12), RADIUS)
    shadow = Image.new("RGBA", (CANVAS, CANVAS), (24, 24, 60, 0))
    shadow.putalpha(shadow_mask.point(lambda v: v * 0.28).filter(ImageFilter.GaussianBlur(14)))
    icon = Image.alpha_composite(icon, shadow)

    # Diagonal gradient fill.
    grad = Image.new("RGBA", (TILE, TILE))
    gp = grad.load()
    for y in range(TILE):
        for x in range(TILE):
            t = (x * 0.35 + y * 0.65) / TILE
            gp[x, y] = tuple(round(a + (b - a) * t) for a, b in zip(TOP_LEFT, BOTTOM_RIGHT)) + (255,)
    fill = Image.new("RGBA", (CANVAS, CANVAS), (0, 0, 0, 0))
    fill.paste(grad, (off, off))
    tile_mask = rounded_mask(CANVAS, box, RADIUS)
    fill.putalpha(tile_mask)
    icon = Image.alpha_composite(icon, fill)

    # Faint rim so the tile reads on light backgrounds.
    inner = rounded_mask(CANVAS, (box[0] + 2, box[1] + 2, box[2] - 2, box[3] - 2), RADIUS - 2)
    rim_alpha = ImageChops.subtract(tile_mask, inner)
    rim = Image.new("RGBA", (CANVAS, CANVAS), (40, 40, 80, 0))
    rim.putalpha(rim_alpha.point(lambda v: v * 0.10))
    icon = Image.alpha_composite(icon, rim)

    # The bird.
    bird = scaled_logo(TILE * BIRD_WIDTH)
    cx = CANVAS / 2 + TILE * BIRD_SHIFT_X
    pos = (round(cx - bird.width / 2), round((CANVAS - bird.height) / 2))
    layer = Image.new("RGBA", (CANVAS, CANVAS), (0, 0, 0, 0))
    layer.paste(bird, pos, bird)
    return Image.alpha_composite(icon, layer)


def bare_bird(size, content):
    """The bird alone, fit into `content` px, centered on a transparent square."""
    w, h = logo.size
    s = content / max(w, h)
    bird = logo.resize((max(1, round(w * s)), max(1, round(h * s))), Image.LANCZOS)
    canvas = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    canvas.paste(bird, ((size - bird.width) // 2, (size - bird.height) // 2), bird)
    return canvas


master = app_icon()
master.resize((512, 512), Image.LANCZOS).save(f"{out}/icon-512.png")
# Sidebar logo (drawn at 20 pt): the bare bird, nearly filling the square.
bare_bird(64, 62).save(f"{out}/icon-64.png")

iconset = f"{out}/kolibri.iconset"
os.makedirs(iconset, exist_ok=True)
for pt in (16, 32, 128, 256, 512):
    for scale in (1, 2):
        px = pt * scale
        name = f"icon_{pt}x{pt}{'@2x' if scale == 2 else ''}.png"
        master.resize((px, px), Image.LANCZOS).save(f"{iconset}/{name}")
