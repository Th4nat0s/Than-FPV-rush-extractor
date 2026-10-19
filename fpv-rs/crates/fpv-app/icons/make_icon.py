"""The app icon: the pilot's rendered logo, on macOS's own tile shape.

macOS draws app icons on a 1024 px canvas as an 824 px tile, centred, with a
185.4 px corner radius and *continuous* corners — curvature that eases in from
the straight edge instead of a circular arc. That is Figma's corner smoothing
at 60%, reproduced here (the figma-squircle construction), so the icon sits in
the Dock and the Finder with the same outline as the system's.

The logo arrives as a finished render on a white page: its own dark rounded
tile, shading and glow included. Only the tile is kept — found by its dark
edges — and it is cut slightly inside its own rim with the macOS shape, so
neither the drawn rim nor the page shows at the edges.
"""
import math
import sys

from PIL import Image, ImageDraw

SIZE, TILE, RADIUS, SMOOTHING = 1024, 824, 185.4, 0.6
SCALE = 4  # supersampling for smooth edges


def bezier(p0, p1, p2, p3, steps=96):
    for i in range(1, steps + 1):
        t = i / steps
        u = 1 - t
        yield tuple(u**3 * a + 3 * u * u * t * b + 3 * u * t * t * c + t**3 * d
                    for a, b, c, d in zip(p0, p1, p2, p3))


def top_right_corner(w, r, s):
    """Points of the smoothed top-right corner of a w-wide square, from the
    top edge to the right edge, in a y-down frame."""
    p = min((1 + s) * r, w / 2)
    arc = 90 * (1 - s)
    arc_section = math.sin(math.radians(arc / 2)) * r * math.sqrt(2)
    alpha = (90 - arc) / 2
    p3_to_p4 = r * math.tan(math.radians(alpha / 2))
    beta = 45 * s
    c = p3_to_p4 * math.cos(math.radians(beta))
    d = c * math.tan(math.radians(beta))
    b = (p - arc_section - c - d) / 3
    a = 2 * b
    start = (w - p, 0.0)
    points = [start]
    # Ease out of the top edge.
    e1 = (start[0] + a + b + c, d)
    points += bezier(start, (start[0] + a, 0), (start[0] + a + b, 0), e1)
    # The circular middle of the corner.
    centre = (w - r, r)
    a0 = math.atan2(e1[1] - centre[1], e1[0] - centre[0])
    e2 = (e1[0] + arc_section, e1[1] + arc_section)
    a1 = math.atan2(e2[1] - centre[1], e2[0] - centre[0])
    for i in range(1, 97):
        t = a0 + (a1 - a0) * i / 96
        points.append((centre[0] + r * math.cos(t), centre[1] + r * math.sin(t)))
    # Ease into the right edge.
    points += bezier(e2, (e2[0] + d, e2[1] + c), (w, e2[1] + b + c + d), (w, e2[1] + a + b + c + d))
    return points


def tile_polygon(w, r, s):
    corner = top_right_corner(w, r, s)
    top_right = corner
    bottom_right = [(x, w - y) for x, y in reversed(corner)]
    bottom_left = [(w - x, w - y) for x, y in corner]
    top_left = [(w - x, y) for x, y in reversed(corner)]
    return top_right + bottom_right + bottom_left + top_left


def mask(w, r, s):
    big = Image.new('L', (w * SCALE, w * SCALE), 0)
    ImageDraw.Draw(big).polygon([(x * SCALE, y * SCALE) for x, y in tile_polygon(w, r, s)], fill=255)
    return big.resize((w, w), Image.LANCZOS)


# How far inside the logo's own tile the macOS outline is cut, as a share of
# the tile: past its dark rim, and past its rounder corners.
INSET = 0.01


def tile_box(gray, threshold=60, run=30):
    """The logo's dark tile: the rows and columns holding a run of dark pixels."""
    w, h = gray.size
    dark = lambda line: sum(1 for v in line if v < threshold) > run
    columns = [x for x in range(w) if dark(gray.getpixel((x, y)) for y in range(0, h, 4))]
    rows = [y for y in range(h) if dark(gray.getpixel((x, y)) for x in range(0, w, 4))]
    return columns[0], rows[0], columns[-1] + 1, rows[-1] + 1


def main(logo, out):
    page = Image.open(logo).convert('RGB')
    tile = page.crop(tile_box(page.convert('L')))
    grown = round(TILE * (1 + 2 * INSET))
    tile = tile.resize((grown, grown), Image.LANCZOS)
    margin = (grown - TILE) // 2
    tile = tile.crop((margin, margin, margin + TILE, margin + TILE))

    icon = Image.new('RGBA', (SIZE, SIZE), (0, 0, 0, 0))
    offset = (SIZE - TILE) // 2
    icon.paste(tile, (offset, offset), mask(TILE, RADIUS, SMOOTHING))
    icon.save(out)


# Usage: python3 make_icon.py logo.png source.png   (needs Pillow), then: make icons
if __name__ == '__main__':
    main(sys.argv[1], sys.argv[2])
