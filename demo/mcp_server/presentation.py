"""Deterministic evidence-bound PowerPoint compiler for the Crudo demo."""

from __future__ import annotations

import hashlib
import json
from pathlib import Path
from typing import Any

from pptx import Presentation
from pptx.dml.color import RGBColor
from pptx.enum.text import PP_ALIGN
from pptx.util import Inches, Pt

ALLOWED_TYPES = {
    "title",
    "summary",
    "findings_table",
    "evidence",
    "recommendations",
    "workflow_trace",
    "security_status",
}

NAVY = RGBColor(16, 37, 63)
BLUE = RGBColor(32, 102, 160)
TEAL = RGBColor(0, 132, 137)
LIGHT = RGBColor(240, 245, 248)
DARK = RGBColor(38, 49, 61)
MUTED = RGBColor(91, 105, 117)


def _require_text(value: Any, field: str) -> str:
    if not isinstance(value, str) or not value.strip():
        raise ValueError(f"{field} must be a non-empty string")
    return value.strip()


def validate_plan(plan: dict[str, Any]) -> dict[str, Any]:
    if not isinstance(plan, dict):
        raise ValueError("presentation plan must be an object")
    title = _require_text(plan.get("title"), "title")
    slides = plan.get("slides")
    if not isinstance(slides, list) or not slides:
        raise ValueError("slides must be a non-empty array")
    normalized: list[dict[str, Any]] = []
    for index, slide in enumerate(slides, 1):
        if not isinstance(slide, dict):
            raise ValueError(f"slides[{index}] must be an object")
        kind = _require_text(slide.get("type"), f"slides[{index}].type")
        if kind not in ALLOWED_TYPES:
            raise ValueError(f"unsupported slide type: {kind}")
        slide_title = _require_text(slide.get("title"), f"slides[{index}].title")
        sources = slide.get("source_ids", [])
        if not isinstance(sources, list) or not all(isinstance(s, str) for s in sources):
            raise ValueError(f"slides[{index}].source_ids must be an array of strings")
        normalized.append({**slide, "type": kind, "title": slide_title, "source_ids": sources})
    review = plan.get("human_review_required", True)
    if review is not True:
        raise ValueError("human_review_required must be true for this demo")
    return {**plan, "title": title, "slides": normalized, "human_review_required": True}


def _add_footer(slide, source_ids: list[str], review: bool) -> None:
    box = slide.shapes.add_textbox(Inches(0.45), Inches(7.05), Inches(12.35), Inches(0.25))
    tf = box.text_frame
    tf.clear()
    p = tf.paragraphs[0]
    p.text = f"Sources: {', '.join(source_ids) if source_ids else 'Synthetic/demo assumption'}  •  Human review required: {str(review).lower()}"
    p.font.size = Pt(8)
    p.font.color.rgb = MUTED


def _add_header(slide, title: str, accent: RGBColor = BLUE) -> None:
    bar = slide.shapes.add_shape(1, Inches(0), Inches(0), Inches(13.333), Inches(0.16))
    bar.fill.solid()
    bar.fill.fore_color.rgb = accent
    bar.line.fill.background()
    box = slide.shapes.add_textbox(Inches(0.55), Inches(0.42), Inches(12.1), Inches(0.55))
    p = box.text_frame.paragraphs[0]
    p.text = title
    p.font.size = Pt(27)
    p.font.bold = True
    p.font.color.rgb = NAVY


def _add_bullets(slide, bullets: list[str], left=0.8, top=1.45, width=11.7, height=5.1) -> None:
    box = slide.shapes.add_textbox(Inches(left), Inches(top), Inches(width), Inches(height))
    tf = box.text_frame
    tf.word_wrap = True
    tf.clear()
    for i, text in enumerate(bullets):
        p = tf.paragraphs[0] if i == 0 else tf.add_paragraph()
        p.text = str(text)
        p.level = 0
        p.font.size = Pt(20 if len(bullets) <= 5 else 16)
        p.font.color.rgb = DARK
        p.space_after = Pt(14)
        p._p.get_or_add_pPr().insert(0, __import__("pptx").oxml.xmlchemy.OxmlElement("a:buChar"))
        p._p.pPr[0].set("char", "•")


def _add_title_slide(prs: Presentation, slide_data: dict[str, Any], plan: dict[str, Any]) -> None:
    slide = prs.slides.add_slide(prs.slide_layouts[6])
    bg = slide.background.fill
    bg.solid(); bg.fore_color.rgb = NAVY
    accent = slide.shapes.add_shape(1, Inches(0), Inches(0), Inches(13.333), Inches(0.24))
    accent.fill.solid(); accent.fill.fore_color.rgb = TEAL; accent.line.fill.background()
    box = slide.shapes.add_textbox(Inches(0.75), Inches(1.35), Inches(11.8), Inches(2.0))
    tf = box.text_frame; tf.clear()
    p = tf.paragraphs[0]; p.text = plan["title"]; p.font.size = Pt(36); p.font.bold = True; p.font.color.rgb = RGBColor(255, 255, 255)
    p2 = tf.add_paragraph(); p2.text = str(slide_data.get("subtitle", "Board briefing • Synthetic demonstration")); p2.font.size = Pt(22); p2.font.color.rgb = RGBColor(194, 215, 229)
    note = slide.shapes.add_textbox(Inches(0.78), Inches(5.65), Inches(11.7), Inches(0.8))
    np = note.text_frame.paragraphs[0]; np.text = "SYNTHETIC DATA / DEMO ASSUMPTIONS — NOT AN OPERATIONAL RECOMMENDATION"; np.font.size = Pt(13); np.font.bold = True; np.font.color.rgb = RGBColor(255, 211, 105)
    _add_footer(slide, slide_data.get("source_ids", []), True)


def render_plan(plan: dict[str, Any], output_path: Path) -> dict[str, Any]:
    plan = validate_plan(plan)
    output_path.parent.mkdir(parents=True, exist_ok=True)
    prs = Presentation()
    prs.slide_width = Inches(13.333)
    prs.slide_height = Inches(7.5)
    for data in plan["slides"]:
        kind = data["type"]
        if kind == "title":
            _add_title_slide(prs, data, plan)
            continue
        slide = prs.slides.add_slide(prs.slide_layouts[6])
        _add_header(slide, data["title"], TEAL if kind == "security_status" else BLUE)
        if kind in {"summary", "recommendations", "workflow_trace", "security_status"}:
            _add_bullets(slide, [str(x) for x in data.get("bullets", [])])
        elif kind == "evidence":
            quote = _require_text(data.get("quote"), f"{data['title']}.quote")
            _add_bullets(slide, [f'“{quote}”', f"Source: {data.get('source', 'not specified')}"])
        elif kind == "findings_table":
            columns = data.get("columns", [])
            rows = data.get("rows", [])
            if not isinstance(columns, list) or not isinstance(rows, list) or not columns:
                raise ValueError("findings_table requires columns and rows")
            table = slide.shapes.add_table(len(rows) + 1, len(columns), Inches(0.6), Inches(1.35), Inches(12.1), Inches(4.9)).table
            for c, value in enumerate(columns):
                cell = table.cell(0, c); cell.text = str(value); cell.fill.solid(); cell.fill.fore_color.rgb = NAVY
                for run in cell.text_frame.paragraphs[0].runs: run.font.bold = True; run.font.color.rgb = RGBColor(255,255,255); run.font.size = Pt(12)
            for r, row in enumerate(rows, 1):
                cells = row.get("cells", []) if isinstance(row, dict) else []
                for c in range(len(columns)):
                    cell = table.cell(r, c); cell.text = str(cells[c] if c < len(cells) else "")
                    cell.fill.solid(); cell.fill.fore_color.rgb = LIGHT if r % 2 else RGBColor(255,255,255)
                    for run in cell.text_frame.paragraphs[0].runs: run.font.color.rgb = DARK; run.font.size = Pt(11)
        _add_footer(slide, data["source_ids"], True)
    prs.save(output_path)
    digest = hashlib.sha256(output_path.read_bytes()).hexdigest()
    return {"output_path": str(output_path), "slide_count": len(prs.slides), "sha256": digest, "human_review_required": True}


def load_plan(path: Path) -> dict[str, Any]:
    return json.loads(path.read_text(encoding="utf-8"))
