"""Local stdio MCP tools for the Crudo industrial demo."""
from __future__ import annotations

import hashlib
import json
import os
import sys
from pathlib import Path
from typing import Any

from presentation import load_plan, render_plan

ROOT = Path(os.environ.get("CRUDO_DEMO_ROOT", Path(__file__).resolve().parents[1])).resolve()
DATA = (ROOT / "data").resolve()
WORKSPACE = (ROOT / "workspace").resolve()
OUTPUT = (ROOT / "output").resolve()
for directory in (DATA, WORKSPACE, OUTPUT):
    directory.mkdir(parents=True, exist_ok=True)

TOOLS = [
    {"name": "extract_document", "description": "Extract a local document with Docling; returns Markdown and status.", "inputSchema": {"type": "object", "properties": {"path": {"type": "string"}}, "required": ["path"]}},
    {"name": "ocr_document", "description": "Run local OCR on an image when PaddleOCR assets are available.", "inputSchema": {"type": "object", "properties": {"path": {"type": "string"}}, "required": ["path"]}},
    {"name": "search_knowledge", "description": "Search the local synthetic knowledge corpus and return citations.", "inputSchema": {"type": "object", "properties": {"query": {"type": "string"}}, "required": ["query"]}},
    {"name": "create_board_presentation", "description": "Create the synthetic truck-fleet board deck directly. Use this for a board-deck request; do not inspect server source or use Bash.", "inputSchema": {"type": "object", "properties": {"output_name": {"type": "string"}}}},
    {"name": "generate_presentation", "description": "Compile an evidence-bound presentation plan into a real editable PPTX. Use only after a plan exists.", "inputSchema": {"type": "object", "properties": {"plan_path": {"type": "string"}, "output_name": {"type": "string"}}, "required": ["plan_path"]}},
    {"name": "verify_presentation", "description": "Verify a generated PPTX contains required content and citations.", "inputSchema": {"type": "object", "properties": {"path": {"type": "string"}}, "required": ["path"]}},
    {"name": "run_code", "description": "Run the fixed safe coding demo; no arbitrary host commands accepted.", "inputSchema": {"type": "object", "properties": {}, "additionalProperties": False}},
    {"name": "analyze_image", "description": "Analyze a local image only when a local vision model is available.", "inputSchema": {"type": "object", "properties": {"path": {"type": "string"}}, "required": ["path"]}},
]


def log(message: str) -> None:
    print(f"industrial-demo: {message}", file=sys.stderr, flush=True)


def safe_path(raw: str, roots=(DATA, WORKSPACE, OUTPUT)) -> Path:
    candidate = Path(raw)
    if not candidate.is_absolute():
        candidate = ROOT / candidate
    candidate = candidate.resolve()
    if not any(candidate == root or root in candidate.parents for root in roots):
        raise ValueError("path is outside the approved demo roots")
    return candidate


def mcp_result(value: Any, text: str | None = None, *, is_error: bool = False) -> dict[str, Any]:
    """Return human-facing MCP text plus machine-readable structured content."""
    if text is None:
        if isinstance(value, dict):
            status = value.get("status")
            if status:
                text = f"Status: {status}"
            elif "output_path" in value:
                text = f"Presentation generated: {value['output_path']} ({value.get('slide_count', '?')} slides)"
            elif "verified" in value:
                text = f"Presentation verification: {'PASS' if value['verified'] else 'FAIL'}"
            else:
                text = "Local tool completed."
        else:
            text = "Local tool completed."
    return {
        "content": [{"type": "text", "text": text}],
        "structuredContent": value,
        "isError": is_error,
    }


def call_tool(name: str, args: dict[str, Any]) -> dict[str, Any]:
    if name == "extract_document":
        path = safe_path(args["path"])
        if not path.is_file(): raise ValueError(f"document not found: {path}")
        out = WORKSPACE / f"{path.stem}.docling.md"
        try:
            docling_pythonpath = os.environ.get("CRUDO_DOCLING_PYTHONPATH")
            if docling_pythonpath:
                for entry in reversed(docling_pythonpath.split(os.pathsep)):
                    sys.path.insert(0, entry)
            from docling.document_converter import DocumentConverter
            result = DocumentConverter().convert(str(path))
            markdown = result.document.export_to_markdown()
            status = "docling_active"
        except Exception as exc:
            markdown = path.read_text(encoding="utf-8", errors="replace") if path.suffix.lower() in {".md", ".txt"} else ""
            status = f"docling_unavailable: {type(exc).__name__}"
        out.write_text(markdown, encoding="utf-8")
        return mcp_result({"status": status, "output_path": str(out), "characters": len(markdown), "sha256": hashlib.sha256(markdown.encode()).hexdigest()})
    if name == "ocr_document":
        path = safe_path(args["path"])
        try:
            from paddleocr import PaddleOCR
            ocr = PaddleOCR(lang="en", device="cpu")
            result = ocr.predict(str(path))
            return mcp_result({"status": "paddleocr_active", "path": str(path), "result": str(result)[:12000]})
        except Exception as exc:
            return mcp_result({"status": "unavailable", "reason": f"{type(exc).__name__}: {exc}", "path": str(path)})
    if name == "search_knowledge":
        query = str(args["query"]).lower()
        hits = []
        for path in DATA.rglob("*.md"):
            text = path.read_text(encoding="utf-8", errors="replace")
            if query in text.lower() or any(word in text.lower() for word in query.split()):
                hits.append({"source": str(path.relative_to(ROOT)), "section": "local-demo", "text": text[:1600], "score": 1.0})
        return mcp_result({"query": query, "hits": hits[:5], "status": "local_keyword_search"})
    if name == "create_board_presentation":
        plan = load_plan(DATA / "truck_fleet_plan.json")
        output_name = str(args.get("output_name", "mangalore_refinery_truck_fleet_board_deck.pptx"))
        if Path(output_name).name != output_name or not output_name.endswith(".pptx"):
            raise ValueError("output_name must be a simple .pptx filename")
        result = render_plan(plan, OUTPUT / output_name)
        return mcp_result(result, f"Board presentation created: {result['output_path']} ({result['slide_count']} slides)\nSynthetic demo assumptions • Human review required")
    if name == "generate_presentation":
        plan_path = safe_path(args["plan_path"], roots=(DATA, WORKSPACE))
        plan = load_plan(plan_path)
        output_name = str(args.get("output_name", "truck_fleet_board_report.pptx"))
        if Path(output_name).name != output_name or not output_name.endswith(".pptx"):
            raise ValueError("output_name must be a simple .pptx filename")
        result = render_plan(plan, OUTPUT / output_name)
        return mcp_result(result, f"Presentation generated: {result['output_path']} ({result['slide_count']} slides)\nSynthetic demo assumptions • Human review required")
    if name == "verify_presentation":
        path = safe_path(args["path"], roots=(OUTPUT,))
        from pptx import Presentation
        prs = Presentation(path)
        text = "\n".join(shape.text for slide in prs.slides for shape in slide.shapes if hasattr(shape, "text"))
        checks = {"readable": True, "slide_count": len(prs.slides), "has_sources": "Sources:" in text, "has_review_notice": "Human review required" in text, "has_synthetic_notice": "SYNTHETIC" in text}
        return mcp_result({"path": str(path), "checks": checks, "verified": all(checks.values())})
    if name == "run_code":
        return mcp_result({"status": "refused", "reason": "no fail-closed sandbox is configured for this demo"})
    if name == "analyze_image":
        return mcp_result({"status": "unavailable", "reason": "no local vision model configured; no engineering claim made"})
    raise ValueError(f"unknown tool: {name}")


def response(request_id: Any, result: Any = None, error: dict[str, Any] | None = None) -> dict[str, Any]:
    body = {"jsonrpc": "2.0", "id": request_id}
    if error is None: body["result"] = result
    else: body["error"] = error
    return body


def send(body: dict[str, Any]) -> None:
    raw = json.dumps(body, ensure_ascii=False).encode()
    sys.stdout.buffer.write(f"Content-Length: {len(raw)}\r\n\r\n".encode() + raw)
    sys.stdout.buffer.flush()


def main() -> None:
    while True:
        headers = {}
        line = sys.stdin.buffer.readline()
        if not line: return
        while line not in (b"\r\n", b"\n"):
            key, _, value = line.decode().partition(":")
            headers[key.lower().strip()] = value.strip()
            line = sys.stdin.buffer.readline()
        length = int(headers.get("content-length", "0"))
        message = json.loads(sys.stdin.buffer.read(length))
        request_id = message.get("id")
        if request_id is None: continue
        method = message.get("method")
        if method == "initialize":
            send(response(request_id, {"protocolVersion": "2025-03-26", "capabilities": {"tools": {}}, "serverInfo": {"name": "industrial-demo", "version": "0.1.0"}}))
        elif method == "tools/list":
            send(response(request_id, {"tools": TOOLS}))
        elif method == "tools/call":
            try:
                params = message.get("params", {})
                send(response(request_id, call_tool(params["name"], params.get("arguments", {}))))
            except Exception as exc:
                log(f"tool failure: {exc}")
                send(response(request_id, error={"code": -32000, "message": str(exc)}))
        else:
            send(response(request_id, error={"code": -32601, "message": f"method not found: {method}"}))


if __name__ == "__main__":
    main()
