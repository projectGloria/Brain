"""Build GloriaGlyph.ttf from src/assets/fonts/gloria-head.svg and render preview PNG."""

from pathlib import Path
import xml.etree.ElementTree as ET

from fontTools.fontBuilder import FontBuilder
from fontTools.pens.areaPen import AreaPen
from fontTools.pens.cu2quPen import Cu2QuPen
from fontTools.pens.recordingPen import RecordingPen
from fontTools.pens.reverseContourPen import ReverseContourPen
from fontTools.pens.transformPen import TransformPen
from fontTools.pens.ttGlyphPen import TTGlyphPen
from fontTools.svgLib.path import SVGPath
from PIL import Image, ImageDraw, ImageFont

REPO_ROOT = Path(__file__).resolve().parent.parent
SVG_PATH = REPO_ROOT / "src" / "assets" / "fonts" / "gloria-head.svg"
TTF_PATH = REPO_ROOT / "src" / "assets" / "fonts" / "GloriaGlyph.ttf"
PREVIEW_PATH = Path(
    r"C:\Users\Nihil\.gemini\antigravity\brain\078878df-2c07-4f9e-a85f-3ee223338ec5\scratch\gloria-font-preview.png"
)

UPM = 1000
ASCENT = 850
DESCENT = -150
CODEPOINT = 0xE800


def parse_svg_contours(svg_file: Path) -> list[tuple[str, str]]:
    tree = ET.parse(svg_file)
    root = tree.getroot()
    contours: list[tuple[str, str]] = []
    for elem in root.iter():
        tag = elem.tag.split("}")[-1]
        if tag == "path":
            d = elem.attrib.get("d", "").strip()
            cls = elem.attrib.get("class", "solid").strip()
            if d:
                contours.append((cls, d))
    return contours


def build_gloria_glyph(svg_file: Path):
    tt_pen = TTGlyphPen(None)
    reverse_pen = ReverseContourPen(tt_pen)

    for cls, d in parse_svg_contours(svg_file):
        # Convert SVG path (Y-down [0..1000]) -> Font coords (Y-up [-150..850])
        # First convert cubic curves to quadratic, transform Y, and record contours.
        rec = RecordingPen()
        t_pen = TransformPen(rec, (1, 0, 0, -1, 0, ASCENT))
        cu2qu_pen = Cu2QuPen(t_pen, max_err=1.0, reverse_direction=False)
        svg_path = SVGPath.fromstring(f'<path d="{d}"/>')
        svg_path.draw(cu2qu_pen)

        # Measure signed area in font Y-up space:
        # TrueType outer contours must be CW (negative area in Cartesian Y-up).
        # TrueType hole contours must be CCW (positive area in Cartesian Y-up).
        area_pen = AreaPen()
        rec.replay(area_pen)
        want_cw = cls != "hole"
        is_cw = area_pen.value < 0

        if is_cw == want_cw:
            rec.replay(tt_pen)
        else:
            rec.replay(reverse_pen)

    return tt_pen.glyph()


def build_notdef_glyph():
    pen = TTGlyphPen(None)
    # Outer box (CW in Y-up)
    pen.moveTo((60, 0))
    pen.lineTo((60, 700))
    pen.lineTo((440, 700))
    pen.lineTo((440, 0))
    pen.closePath()
    # Inner cutout (CCW in Y-up)
    pen.moveTo((110, 50))
    pen.lineTo((390, 50))
    pen.lineTo((390, 650))
    pen.lineTo((110, 650))
    pen.closePath()
    return pen.glyph()


def build_font():
    fb = FontBuilder(UPM, isTTF=True)
    fb.setupGlyphOrder([".notdef", "space", "gloria"])
    fb.setupCharacterMap({0x0020: "space", CODEPOINT: "gloria"})

    empty_pen = TTGlyphPen(None)
    fb.setupGlyf(
        {
            ".notdef": build_notdef_glyph(),
            "space": empty_pen.glyph(),
            "gloria": build_gloria_glyph(SVG_PATH),
        }
    )

    metrics = {
        ".notdef": (500, 60),
        "space": (500, 0),
        "gloria": (1000, 104),
    }
    fb.setupHorizontalMetrics(metrics)
    fb.setupHorizontalHeader(ascent=ASCENT, descent=DESCENT, lineGap=0)
    fb.setupNameTable(
        {
            "familyName": "GloriaGlyph",
            "styleName": "Regular",
            "uniqueFontIdentifier": "GloriaGlyph-Regular-1.000",
            "fullName": "GloriaGlyph Regular",
            "psName": "GloriaGlyph-Regular",
            "version": "Version 1.000",
        }
    )
    fb.setupOS2(
        sTypoAscender=ASCENT,
        sTypoDescender=DESCENT,
        sTypoLineGap=0,
        usWinAscent=ASCENT,
        usWinDescent=abs(DESCENT),
        fsType=0,
    )
    fb.setupPost()

    TTF_PATH.parent.mkdir(parents=True, exist_ok=True)
    fb.save(TTF_PATH)
    print(f"Built {TTF_PATH} ({TTF_PATH.stat().st_size} bytes)")


def render_preview():
    PREVIEW_PATH.parent.mkdir(parents=True, exist_ok=True)
    img = Image.new("RGB", (980, 580), (14, 14, 16))
    draw = ImageDraw.Draw(img)

    # Top half: dark background; bottom half: light background
    draw.rectangle([(0, 290), (980, 580)], fill=(246, 246, 248))

    # Load GloriaGlyph + fallback Consolas for text comparison
    sizes = [14, 18, 24, 48, 128]
    glyph_char = chr(CODEPOINT)

    def draw_panel(y_start: int, fg: tuple[int, int, int]):
        y = y_start + 18
        for sz in sizes:
            gfont = ImageFont.truetype(str(TTF_PATH), sz)
            try:
                tfont = ImageFont.truetype(r"C:\Windows\Fonts\consola.ttf", sz)
            except OSError:
                tfont = ImageFont.load_default()

            if sz <= 48:
                draw.text((24, y), glyph_char, font=gfont, fill=fg)
                draw.text((24 + sz + 10, y), f"gloria-init-brain  ({sz}px)", font=tfont, fill=fg)
                y += max(sz + 14, 28)
            else:
                # Large 128px showcase on the right side
                draw.text((760, y_start + 70), glyph_char, font=gfont, fill=fg)

    draw_panel(0, (255, 255, 255))
    draw_panel(290, (16, 16, 20))
    img.save(PREVIEW_PATH)
    print(f"Saved preview to {PREVIEW_PATH}")


if __name__ == "__main__":
    build_font()
    render_preview()
