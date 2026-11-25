"""Build GloriaMono.ttf from Windows CascadiaCode.ttf (OFL) with Gloria head (U+E800) and ::gloria ligature."""

from __future__ import annotations

from pathlib import Path
import xml.etree.ElementTree as ET

from fontTools.pens.areaPen import AreaPen
from fontTools.pens.cu2quPen import Cu2QuPen
from fontTools.pens.recordingPen import RecordingPen
from fontTools.pens.reverseContourPen import ReverseContourPen
from fontTools.pens.transformPen import TransformPen
from fontTools.pens.ttGlyphPen import TTGlyphPen
from fontTools.svgLib.path import SVGPath
from fontTools.ttLib import TTFont
from fontTools.ttLib.tables import otTables
from fontTools.varLib.instancer import instantiateVariableFont
from PIL import Image, ImageDraw, ImageFont

REPO_ROOT = Path(__file__).resolve().parent.parent
SOURCE_TTF = Path(r"C:\Windows\Fonts\CascadiaCode.ttf")
HEAD_SVG_PATH = REPO_ROOT / "src" / "assets" / "fonts" / "gloria-head.svg"
OUT_TTF_PATH = REPO_ROOT / "src" / "assets" / "fonts" / "GloriaMono.ttf"
PREVIEW_PATH = Path(
    r"C:\Users\Nihil\.gemini\antigravity\brain\078878df-2c07-4f9e-a85f-3ee223338ec5\gloria-mono-preview.png"
)

HEAD_CODEPOINT = 0xE800


def build_gloria_glyph_for_cascadia(svg_file: Path, adv_width: int = 1200):
    """Import gloria-head.svg (1000x1000 Y-down) into Cascadia's 2048-UPM Y-up coordinate space."""
    # Scale 1000 -> 1140 units tall/wide, centered horizontally in advanceWidth=1200 (x: 30..1170),
    # vertically spanning y: 120..1260 (cap height in Cascadia is ~1434, x-height ~1020).
    scale = 1.14
    tx = (adv_width - 1000 * scale) / 2.0
    ty = 1260.0  # y_font = ty - y_svg * scale -> y_svg=0 -> 1260, y_svg=1000 -> 120

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
        cu2qu_pen = Cu2QuPen(t_pen, max_err=1.5, reverse_direction=False)
        SVGPath.fromstring(f'<path d="{d}"/>').draw(cu2qu_pen)

        area = AreaPen()
        rec.replay(area)
        want_cw = cls != "hole"
        is_cw = area.value < 0
        if is_cw == want_cw:
            rec.replay(tt_pen)
        else:
            rec.replay(rev_pen)

    return tt_pen.glyph()


def add_gloria_ligature(font: TTFont, target_glyph: str = "gloria"):
    """Add OpenType LigatureSubst lookup: ::gloria -> gloria under liga and calt."""
    if "GSUB" not in font:
        return
    gsub = font["GSUB"].table
    cmap = font.getBestCmap()
    seq = [cmap[ord(ch)] for ch in "::gloria"]

    # Build Type 4 LigatureSubst subtable (ligatures maps first glyph -> list[Ligature])
    subtable = otTables.LigatureSubst()
    subtable.Format = 1
    lig = otTables.Ligature()
    lig.Component = seq[1:]
    lig.LigGlyph = target_glyph
    subtable.ligatures = {seq[0]: [lig]}

    lookup = otTables.Lookup()
    lookup.LookupType = 4
    lookup.LookupFlag = 0
    lookup.SubTable = [subtable]
    lookup.SubTableCount = 1

    new_lookup_idx = len(gsub.LookupList.Lookup)
    gsub.LookupList.Lookup.append(lookup)
    gsub.LookupList.LookupCount = len(gsub.LookupList.Lookup)

    # Attach to existing liga / calt features (and create liga if absent)
    attached = False
    for feat_rec in gsub.FeatureList.FeatureRecord:
        if feat_rec.FeatureTag in ("liga", "calt"):
            feat_rec.Feature.LookupListIndex.append(new_lookup_idx)
            feat_rec.Feature.LookupCount = len(feat_rec.Feature.LookupListIndex)
            attached = True

    if not attached:
        feat = otTables.Feature()
        feat.FeatureParams = None
        feat.LookupListIndex = [new_lookup_idx]
        feat.LookupCount = 1
        rec = otTables.FeatureRecord()
        rec.FeatureTag = "liga"
        rec.Feature = feat
        feat_idx = len(gsub.FeatureList.FeatureRecord)
        gsub.FeatureList.FeatureRecord.append(rec)
        gsub.FeatureList.FeatureCount = len(gsub.FeatureList.FeatureRecord)
        for scr_rec in gsub.ScriptList.ScriptRecord:
            scr = scr_rec.Script
            if scr.DefaultLangSys:
                scr.DefaultLangSys.FeatureIndex.append(feat_idx)
                scr.DefaultLangSys.FeatureCount = len(scr.DefaultLangSys.FeatureIndex)


def update_name_table(font: TTFont):
    name_table = font["name"]
    replacements = {
        1: "Gloria Mono",
        2: "Regular",
        3: "GloriaMono-Regular-1.000",
        4: "Gloria Mono Regular",
        5: "Version 1.000",
        6: "GloriaMono-Regular",
        16: "Gloria Mono",
        17: "Regular",
    }
    for rec in name_table.names:
        if rec.nameID in replacements:
            rec.string = replacements[rec.nameID]


def build_font():
    tt = TTFont(str(SOURCE_TTF))
    # Instantiate static Regular (wght=400) from variable CascadiaCode.ttf
    static_tt = instantiateVariableFont(tt, {"wght": 400})
    if "DSIG" in static_tt:
        del static_tt["DSIG"]

    adv_width = static_tt["hmtx"]["A"][0]
    gloria_glyph = build_gloria_glyph_for_cascadia(HEAD_SVG_PATH, adv_width=adv_width)

    glyph_order = list(static_tt.getGlyphOrder())
    if "gloria" not in glyph_order:
        glyph_order.append("gloria")
        static_tt.setGlyphOrder(glyph_order)

    gloria_glyph.recalcBounds(None)
    static_tt["glyf"]["gloria"] = gloria_glyph
    lsb = gloria_glyph.xMin if gloria_glyph.numberOfContours > 0 else 30
    static_tt["hmtx"]["gloria"] = (adv_width, lsb)

    if "GDEF" in static_tt and getattr(static_tt["GDEF"].table, "GlyphClassDef", None):
        static_tt["GDEF"].table.GlyphClassDef.classDefs["gloria"] = 1

    for table in static_tt["cmap"].tables:
        if table.isUnicode():
            table.cmap[HEAD_CODEPOINT] = "gloria"

    add_gloria_ligature(static_tt, target_glyph="gloria")
    update_name_table(static_tt)

    OUT_TTF_PATH.parent.mkdir(parents=True, exist_ok=True)
    static_tt.save(str(OUT_TTF_PATH))
    print(f"Built {OUT_TTF_PATH} ({OUT_TTF_PATH.stat().st_size} bytes, {len(glyph_order)} glyphs)")


def render_preview():
    PREVIEW_PATH.parent.mkdir(parents=True, exist_ok=True)
    img = Image.new("RGB", (1120, 520), (8, 8, 10))
    draw = ImageDraw.Draw(img)
    draw.rectangle([(0, 260), (1120, 520)], fill=(247, 247, 245))

    def draw_block(y0: int, fg: tuple[int, int, int]):
        f36 = ImageFont.truetype(str(OUT_TTF_PATH), 36)
        f24 = ImageFont.truetype(str(OUT_TTF_PATH), 24)
        f16 = ImageFont.truetype(str(OUT_TTF_PATH), 16)
        head = chr(HEAD_CODEPOINT)
        draw.text((32, y0 + 22), f"{head} gloria-init-brain   [U+E800]", font=f36, fill=fg)
        draw.text((32, y0 + 82), f"{head} gloria-github-release  -> != === ::gloria", font=f24, fill=fg)
        draw.text((32, y0 + 132), "0123456789  ABCDEFGHIJKLMNOPQRSTUVWXYZ  ÇĞIÖŞÜ çğıöşü", font=f16, fill=fg)
        draw.text((32, y0 + 174), f"{head} Gloria Mono Regular (16px) — Cascadia Code OFL + Gloria U+E800", font=f16, fill=fg)

    draw_block(0, (255, 255, 255))
    draw_block(260, (14, 14, 18))
    img.save(PREVIEW_PATH)
    print(f"Saved preview to {PREVIEW_PATH}")


if __name__ == "__main__":
    build_font()
    render_preview()
