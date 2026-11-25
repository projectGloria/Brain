"""Build GloriaShodo.ttf from master SIL OFL YujiSyuku-Regular.ttf + dry-brush bristle accents + Turkish + Gloria U+E800."""

from __future__ import annotations

import hashlib
import math
from pathlib import Path
import urllib.request
import xml.etree.ElementTree as ET

from fontTools import subset
from fontTools.pens.areaPen import AreaPen
from fontTools.pens.cu2quPen import Cu2QuPen
from fontTools.pens.recordingPen import RecordingPen
from fontTools.pens.reverseContourPen import ReverseContourPen
from fontTools.pens.transformPen import TransformPen
from fontTools.pens.ttGlyphPen import TTGlyphPen
from fontTools.svgLib.path import SVGPath
from fontTools.ttLib import TTFont
from fontTools.ttLib.tables._g_l_y_f import Glyph, GlyphComponent
from PIL import Image, ImageDraw, ImageFont

REPO_ROOT = Path(__file__).resolve().parent.parent
HEAD_SVG_PATH = REPO_ROOT / "src" / "assets" / "fonts" / "gloria-head.svg"
OUT_TTF_PATH = REPO_ROOT / "src" / "assets" / "fonts" / "GloriaShodo.ttf"
BASE_CACHE_PATH = Path(
    r"C:\Users\Nihil\.gemini\antigravity\brain\078878df-2c07-4f9e-a85f-3ee223338ec5\scratch\ofl-bases\YujiSyuku-Regular.ttf"
)
BASE_URL = "https://github.com/google/fonts/raw/main/ofl/yujisyuku/YujiSyuku-Regular.ttf"
PREVIEW_PATH = Path(
    r"C:\Users\Nihil\.gemini\antigravity\brain\078878df-2c07-4f9e-a85f-3ee223338ec5\gloria-shodo-preview.png"
)

HEAD_CODEPOINT = 0xE800


def _seed_val(key: str) -> float:
    h = hashlib.sha256(key.encode("utf-8")).digest()
    return int.from_bytes(h[:4], "little") / 2**32


def ensure_base_font() -> Path:
    if BASE_CACHE_PATH.is_file():
        return BASE_CACHE_PATH
    local_fallback = REPO_ROOT / "scripts" / ".cache" / "YujiSyuku-Regular.ttf"
    if local_fallback.is_file():
        return local_fallback
    local_fallback.parent.mkdir(parents=True, exist_ok=True)
    print(f"Downloading {BASE_URL}...")
    urllib.request.urlretrieve(BASE_URL, str(local_fallback))
    return local_fallback


def _emit_cw_poly(tt_pen: TTGlyphPen, pts: list[tuple[float, float]]):
    if len(pts) < 3:
        return
    rec = RecordingPen()
    rec.moveTo((round(pts[0][0]), round(pts[0][1])))
    for x, y in pts[1:]:
        rec.lineTo((round(x), round(y)))
    rec.closePath()
    area = AreaPen()
    rec.replay(area)
    if abs(area.value) < 5:
        return
    if area.value < 0:
        rec.replay(tt_pen)
    else:
        rec.replay(ReverseContourPen(tt_pen))


def _bristle_hair(
    tt_pen: TTGlyphPen,
    origin: tuple[float, float],
    tx: float,
    ty: float,
    length: float,
    half_w: float,
    curve_bend: float = 0.0,
):
    """Emit a slender tapered CW dry-brush hair polygon."""
    nx, ny = -ty, tx
    ox, oy = origin
    p0 = (ox - nx * half_w, oy - ny * half_w)
    p1 = (
        ox + tx * length * 0.48 - nx * half_w * 0.42 + nx * curve_bend * 0.4,
        oy + ty * length * 0.48 - ny * half_w * 0.42 + ny * curve_bend * 0.4,
    )
    tip = (ox + tx * length + nx * curve_bend, oy + ty * length + ny * curve_bend)
    p3 = (
        ox + tx * length * 0.48 + nx * half_w * 0.42 + nx * curve_bend * 0.4,
        oy + ty * length * 0.48 + ny * half_w * 0.42 + ny * curve_bend * 0.4,
    )
    p4 = (ox + nx * half_w, oy + ny * half_w)
    _emit_cw_poly(tt_pen, [p0, p1, tip, p3, p4])


def _extract_contours_points(glyf_table, glyph_name: str) -> list[list[tuple[float, float]]]:
    """Flatten a simple glyf glyph's contours into dense polyline point lists."""
    g = glyf_table[glyph_name]
    if g.isComposite() or g.numberOfContours <= 0:
        return []
    rec = RecordingPen()
    g.draw(rec, glyf_table)
    contours: list[list[tuple[float, float]]] = []
    cur: list[tuple[float, float]] = []

    def add_pt(p):
        if not cur or math.hypot(p[0] - cur[-1][0], p[1] - cur[-1][1]) > 0.5:
            cur.append((float(p[0]), float(p[1])))

    for op, args in rec.value:
        if op == "moveTo":
            if cur:
                contours.append(cur)
                cur = []
            add_pt(args[0])
        elif op == "lineTo":
            add_pt(args[0])
        elif op == "qCurveTo":
            # Sample quadratic spline chain cleanly
            p0 = cur[-1] if cur else (0.0, 0.0)
            pts = list(args)
            if pts[-1] is None:
                continue
            for idx in range(len(pts) - 1):
                c = pts[idx]
                if idx < len(pts) - 2:
                    nxt = pts[idx + 1]
                    end = ((c[0] + nxt[0]) * 0.5, (c[1] + nxt[1]) * 0.5)
                else:
                    end = pts[-1]
                for step in range(1, 5):
                    t = step / 4.0
                    mt = 1.0 - t
                    px = mt * mt * p0[0] + 2 * mt * t * c[0] + t * t * end[0]
                    py = mt * mt * p0[1] + 2 * mt * t * c[1] + t * t * end[1]
                    add_pt((px, py))
                p0 = end
        elif op in ("closePath", "endPath"):
            if len(cur) >= 3:
                contours.append(cur)
            cur = []
    if len(cur) >= 3:
        contours.append(cur)
    return contours


def enhance_glyph_with_brushes(glyf_table, glyph_name: str, is_upper: bool):
    """Append subtle outer dry-brush bristle wisps and terminal spikes around a YujiSyuku glyph."""
    g = glyf_table[glyph_name]
    if g.isComposite() or g.numberOfContours <= 0:
        return

    contours = _extract_contours_points(glyf_table, glyph_name)
    if not contours:
        return

    # Copy existing glyph contours verbatim first via TTGlyphPen
    pen = TTGlyphPen(glyf_table)
    g.draw(pen, glyf_table)

    seed = _seed_val(glyph_name)
    scale = 1.0 if is_upper else 0.78

    for c_idx, pts in enumerate(contours):
        # Measure signed area: only feather outer contours (CW => negative area in Y-up)
        rec_c = RecordingPen()
        rec_c.moveTo(pts[0])
        for p in pts[1:]:
            rec_c.lineTo(p)
        rec_c.closePath()
        area = AreaPen()
        rec_c.replay(area)
        if area.value >= 0:
            continue

        n = len(pts)
        if n < 6:
            continue

        # Compute cumulative perimeter and detect sharp outer terminal corners
        cum = [0.0]
        for i in range(1, n):
            cum.append(cum[-1] + math.hypot(pts[i][0] - pts[i - 1][0], pts[i][1] - pts[i - 1][1]))
        perim = cum[-1] + math.hypot(pts[0][0] - pts[-1][0], pts[0][1] - pts[-1][1])
        if perim < 180:
            continue

        # 1) Sharp outer terminal flares (like serif tips, stroke ends, R tail, A apex/feet, L foot)
        last_flare_pos = -999.0
        for i in range(n):
            p_prev = pts[(i - 2) % n]
            p_curr = pts[i]
            p_next = pts[(i + 2) % n]
            v1x, v1y = p_curr[0] - p_prev[0], p_curr[1] - p_prev[1]
            v2x, v2y = p_next[0] - p_curr[0], p_next[1] - p_curr[1]
            l1, l2 = math.hypot(v1x, v1y), math.hypot(v2x, v2y)
            if l1 < 6.0 or l2 < 6.0:
                continue
            u1x, u1y = v1x / l1, v1y / l1
            u2x, u2y = v2x / l2, v2y / l2
            dot = max(-1.0, min(1.0, u1x * u2x + u1y * u2y))
            cross = u1x * u2y - u1y * u2x
            # In CW outer contour, convex turns have cross < 0 (right turn)
            if cross < -0.28 and dot < 0.55 and (cum[i] - last_flare_pos) > 75.0 * scale:
                last_flare_pos = cum[i]
                # Outward bisector tangent pointing away from corner
                bx, by = u1x - u2x, u1y - u2y
                bl = math.hypot(bx, by)
                if bl < 1e-4:
                    continue
                bx /= bl
                by /= bl
                bnx, bny = -by, bx
                # Emit 3 fine dry-brush bristle hairs fanning from this terminal tip
                for k in range(3):
                    frac = k - 1
                    ang = frac * 0.16 + math.sin(seed * 29.0 + i + k) * 0.04
                    ca, sa = math.cos(ang), math.sin(ang)
                    htx = bx * ca - by * sa
                    hty = bx * sa + by * ca
                    hlen = (30.0 + 22.0 * abs(math.cos(seed * 17.0 + i * 2 + k))) * scale
                    hw = (3.4 if k == 1 else 2.5) * scale
                    ox = p_curr[0] - bx * 10.0 + bnx * (frac * 5.0 * scale)
                    oy = p_curr[1] - by * 10.0 + bny * (frac * 5.0 * scale)
                    _bristle_hair(pen, (ox, oy), htx, hty, hlen + 10.0, hw, curve_bend=frac * 4.5 * scale)

        # 2) Delicate tangential flyaway brush wisps along smooth edges around the character
        num_wisps = 5 if is_upper else 3
        for w_i in range(num_wisps):
            target_d = perim * ((w_i + 0.35 + 0.3 * ((seed * (w_i + 1)) % 1.0)) / num_wisps)
            idx = 0
            for j, d in enumerate(cum):
                if d >= target_d:
                    idx = j
                    break
            p_prev = pts[(idx - 1) % n]
            p_curr = pts[idx]
            p_next = pts[(idx + 1) % n]
            tx, ty = p_next[0] - p_prev[0], p_next[1] - p_prev[1]
            tl = math.hypot(tx, ty)
            if tl < 2.0:
                continue
            tx /= tl
            ty /= tl
            # Outward normal for CW contour in Y-up: (-ty, tx) points to the left = OUTSIDE CW!
            nx, ny = -ty, tx
            # Skim slightly outward at ~8 degrees along tangent
            ang = 0.14 if (w_i % 2 == 0) else -0.14
            sign_t = 1.0 if (w_i % 2 == 0) else -1.0
            stx, sty = tx * sign_t, ty * sign_t
            ca, sa = math.cos(ang), math.sin(ang)
            wtx = stx * ca - sty * sa
            wty = stx * sa + sty * ca

            ox = p_curr[0] - nx * 2.5
            oy = p_curr[1] - ny * 2.5
            wlen = (34.0 + 20.0 * abs(math.sin(seed * 41.0 + w_i * 5.3))) * scale
            _bristle_hair(pen, (ox, oy), wtx, wty, wlen, half_w=2.5 * scale, curve_bend=3.0 * scale)

    glyf_table[glyph_name] = pen.glyph()


def _create_brush_breve_glyph(glyf_table) -> Glyph:
    """Synthesize a shodo brush breve diacritic matching YujiSyuku's stroke weight."""
    pen = TTGlyphPen(glyf_table)
    pts = [
        (36, 758), (84, 774), (112, 728), (176, 706), (242, 728), (274, 772), (310, 758),
        (286, 712), (228, 672), (176, 666), (120, 674), (62, 712),
    ]
    _emit_cw_poly(pen, pts)
    g = pen.glyph()
    g.recalcBounds(glyf_table)
    return g


def _create_brush_dot_glyph(glyf_table) -> Glyph:
    """Extract the dot contour from YujiSyuku's 'i' glyph centered around x=140, y=720."""
    i_contours = _extract_contours_points(glyf_table, "i")
    # Pick highest contour (the dot!)
    dot_c = max(i_contours, key=lambda c: sum(p[1] for p in c) / len(c))
    min_x = min(p[0] for p in dot_c)
    max_x = max(p[0] for p in dot_c)
    min_y = min(p[1] for p in dot_c)
    cx = (min_x + max_x) * 0.5
    pen = TTGlyphPen(glyf_table)
    shifted = [(x - cx + 100.0, y - min_y + 20.0) for x, y in dot_c]
    _emit_cw_poly(pen, shifted)
    g = pen.glyph()
    g.recalcBounds(glyf_table)
    return g


def _make_composite(base_name: str, base_dx: int, base_dy: int, mark_name: str, mark_dx: int, mark_dy: int) -> Glyph:
    g = Glyph()
    g.numberOfContours = -1
    c1 = GlyphComponent()
    c1.glyphName = base_name
    c1.x = base_dx
    c1.y = base_dy
    c1.flags = 0x0002 | 0x0004  # ARGS_ARE_XY_VALUES | ROUND_XY_TO_GRID
    c2 = GlyphComponent()
    c2.glyphName = mark_name
    c2.x = mark_dx
    c2.y = mark_dy
    c2.flags = 0x0002 | 0x0004
    g.components = [c1, c2]
    return g


def add_turkish_composites(tt: TTFont):
    glyf = tt["glyf"]
    hmtx = tt["hmtx"]
    glyph_order = list(tt.getGlyphOrder())

    for gname in ("gloriaBreve", "gloriaDot"):
        if gname not in glyph_order:
            glyph_order.append(gname)

    breve_g = _create_brush_breve_glyph(glyf)
    dot_g = _create_brush_dot_glyph(glyf)
    glyf["gloriaBreve"] = breve_g
    glyf["gloriaDot"] = dot_g
    hmtx["gloriaBreve"] = (340, breve_g.xMin)
    hmtx["gloriaDot"] = (200, dot_g.xMin)

    def center_x(gname: str) -> float:
        gg = glyf[gname]
        gg.recalcBounds(glyf)
        return (gg.xMin + gg.xMax) * 0.5

    # cedilla bbox in YujiSyuku: x: 102..289 (center ~195), y: -190..35
    ced_cx = center_x("cedilla") if "cedilla" in glyf else 195.0
    breve_cx = (breve_g.xMin + breve_g.xMax) * 0.5
    breve_y0 = breve_g.yMin
    dot_cx = (dot_g.xMin + dot_g.xMax) * 0.5
    dot_y0 = dot_g.yMin

    # Ensure bounds are calculated on bases
    for b in ("G", "g", "I", "S", "s"):
        glyf[b].recalcBounds(glyf)

    specs = [
        ("Gbreve", 0x011E, "G", "gloriaBreve", round(center_x("G") - breve_cx), round(glyf["G"].yMax + 48 - breve_y0)),
        ("gbreve", 0x011F, "g", "gloriaBreve", round(center_x("g") - breve_cx), round(glyf["g"].yMax + 46 - breve_y0)),
        ("Idotaccent", 0x0130, "I", "gloriaDot", round(center_x("I") - dot_cx), round(glyf["I"].yMax + 54 - dot_y0)),
        ("Scedilla", 0x015E, "S", "cedilla", round(center_x("S") - ced_cx), 0),
        ("scedilla", 0x015F, "s", "cedilla", round(center_x("s") - ced_cx), 0),
    ]

    cmap_tables = [t for t in tt["cmap"].tables if t.isUnicode()]
    for name, cp, base, mark, mdx, mdy in specs:
        if name not in glyph_order:
            glyph_order.append(name)
        comp = _make_composite(base, 0, 0, mark, mdx, mdy)
        glyf[name] = comp
        comp.recalcBounds(glyf)
        hmtx[name] = hmtx[base]
        for t in cmap_tables:
            t.cmap[cp] = name

    tt.setGlyphOrder(glyph_order)


def build_gloria_head_glyph(svg_file: Path, adv_width: int = 860):
    """Import gloria-head.svg scaled cleanly to YujiSyuku's 1000-UPM cap height (~740 tall)."""
    scale = 0.76
    tx = (adv_width - 1000 * scale) * 0.5
    ty = 720.0  # y_font = 720 - y_svg * 0.76 => y: -40 .. 720

    tree = ET.parse(svg_file)
    root = tree.getroot()
    tt_pen = TTGlyphPen(None)
    rev_pen = ReverseContourPen(tt_pen)

    for elem in root.iter():
        if elem.tag.split("}")[-1] != "path":
            continue
        d = elem.attrib.get("d", "").strip()
        cls = elem.attrib.get("class", "solid").strip()
        if not d:
            continue
        rec = RecordingPen()
        t_pen = TransformPen(rec, (scale, 0, 0, -scale, tx, ty))
        cu2qu_pen = Cu2QuPen(t_pen, max_err=1.0, reverse_direction=False)
        SVGPath.fromstring(f'<path d="{d}"/>').draw(cu2qu_pen)

        area = AreaPen()
        rec.replay(area)
        want_cw = cls != "hole"
        is_cw = area.value < 0
        if is_cw == want_cw:
            rec.replay(tt_pen)
        else:
            rec.replay(rev_pen)
    g = tt_pen.glyph()
    g.recalcBounds(None)
    return g


def update_name_table(tt: TTFont):
    replacements = {
        1: "Gloria Shodo",
        2: "Regular",
        3: "GloriaShodo-Regular-1.000",
        4: "Gloria Shodo Regular",
        5: "Version 1.000",
        6: "GloriaShodo-Regular",
        16: "Gloria Shodo",
        17: "Regular",
    }
    for rec in tt["name"].names:
        if rec.nameID in replacements:
            rec.string = replacements[rec.nameID]


def build_font():
    base_path = ensure_base_font()
    raw_tt = TTFont(str(base_path))

    # Collect target unicodes to keep
    keep_cps = set(range(0x0020, 0x007F))
    keep_cps.update(range(0x00A0, 0x0100))
    keep_cps.update([0x011E, 0x011F, 0x0130, 0x0131, 0x015E, 0x015F, 0x2013, 0x2014, 0x2018, 0x2019, 0x201C, 0x201D, 0x2026])

    options = subset.Options()
    options.layout_features = ["*"]
    options.name_IDs = ["*"]
    options.glyph_names = True
    options.drop_tables += ["vhea", "vmtx"]
    subsetter = subset.Subsetter(options=options)
    subsetter.populate(unicodes=list(keep_cps))
    subsetter.subset(raw_tt)

    # Add Turkish composites first (so base glyphs get enhanced too, or enhance bases before composites!)
    glyf = raw_tt["glyf"]
    cmap = raw_tt.getBestCmap()

    # Enhance A-Z, a-z, 0-9 with authentic dry-brush terminal flares & outer edge wisps
    for cp in list(range(ord("A"), ord("Z") + 1)):
        gname = cmap.get(cp)
        if gname and gname in glyf:
            enhance_glyph_with_brushes(glyf, gname, is_upper=True)

    for cp in list(range(ord("a"), ord("z") + 1)) + list(range(ord("0"), ord("9") + 1)) + [0x0131]:
        gname = cmap.get(cp)
        if gname and gname in glyf:
            enhance_glyph_with_brushes(glyf, gname, is_upper=False)

    add_turkish_composites(raw_tt)

    # Add Gloria head at U+E800
    glyph_order = list(raw_tt.getGlyphOrder())
    if "gloria" not in glyph_order:
        glyph_order.append("gloria")
        raw_tt.setGlyphOrder(glyph_order)

    adv_w = 860
    head_g = build_gloria_head_glyph(HEAD_SVG_PATH, adv_width=adv_w)
    glyf["gloria"] = head_g
    raw_tt["hmtx"]["gloria"] = (adv_w, head_g.xMin)
    for t in raw_tt["cmap"].tables:
        if t.isUnicode():
            t.cmap[HEAD_CODEPOINT] = "gloria"

    update_name_table(raw_tt)

    OUT_TTF_PATH.parent.mkdir(parents=True, exist_ok=True)
    raw_tt.save(str(OUT_TTF_PATH))
    print(f"Built {OUT_TTF_PATH} ({OUT_TTF_PATH.stat().st_size} bytes, {len(glyph_order)} glyphs)")


def render_preview():
    PREVIEW_PATH.parent.mkdir(parents=True, exist_ok=True)
    img = Image.new("RGB", (1320, 880), (8, 8, 10))
    draw = ImageDraw.Draw(img)
    draw.rectangle([(0, 440), (1320, 880)], fill=(247, 247, 245))

    def draw_block(y0: int, fg: tuple[int, int, int]):
        f_xl = ImageFont.truetype(str(OUT_TTF_PATH), 96)
        f_lg = ImageFont.truetype(str(OUT_TTF_PATH), 42)
        f_md = ImageFont.truetype(str(OUT_TTF_PATH), 34)
        f_sm = ImageFont.truetype(str(OUT_TTF_PATH), 26)

        draw.text((38, y0 + 22), f"{chr(HEAD_CODEPOINT)} GLORIA   BRAINZ", font=f_xl, fill=fg)
        draw.text((40, y0 + 145), "ABCDEFGHIJKLMNOPQRSTUVWXYZ", font=f_lg, fill=fg)
        draw.text((40, y0 + 205), "abcdefghijklmnopqrstuvwxyz", font=f_lg, fill=fg)
        draw.text((40, y0 + 266), "0123456789   ÇĞIÖŞÜ  çğıöşü", font=f_md, fill=fg)
        draw.text((40, y0 + 324), "The quick brown fox jumps over the lazy dog. (!?:;/-+)", font=f_sm, fill=fg)
        draw.text((40, y0 + 368), "Gloria Shodo — Yuji Syuku OFL + Dry-Brush + Gloria U+E800", font=f_sm, fill=fg)

    draw_block(0, (255, 255, 255))
    draw_block(440, (14, 14, 18))
    img.save(PREVIEW_PATH)
    print(f"Saved preview to {PREVIEW_PATH}")


if __name__ == "__main__":
    build_font()
    render_preview()
