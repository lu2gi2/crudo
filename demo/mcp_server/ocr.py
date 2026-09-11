"""Local OCR capability adapter for the industrial demo MCP server."""
from __future__ import annotations

import hashlib
import tempfile
from pathlib import Path
from typing import Any, Callable, Iterable


class OcrUnavailable(RuntimeError):
    """Raised when the local OCR engine or its model assets are unavailable."""


def _default_factory() -> Any:
    try:
        from paddleocr import PaddleOCR
    except Exception as exc:  # pragma: no cover - depends on optional local assets
        raise OcrUnavailable(f"PaddleOCR is unavailable: {type(exc).__name__}: {exc}") from exc

    return PaddleOCR(lang="en", device="cpu")


def _payload_for(raw: Any) -> dict[str, Any]:
    value = getattr(raw, "json", None)
    payload = value() if callable(value) else value
    if not isinstance(payload, dict):
        return {}
    result = payload.get("res", payload)
    return result if isinstance(result, dict) else {}


def _pdf_page_images(path: Path) -> Iterable[tuple[int, Path]]:
    """Render PDF pages locally when the optional PyMuPDF dependency exists."""
    try:
        import fitz
    except Exception as exc:  # pragma: no cover - optional dependency
        raise OcrUnavailable(
            f"scanned PDF rendering is unavailable: {type(exc).__name__}: {exc}"
        ) from exc

    temporary = tempfile.TemporaryDirectory(prefix="crudo-ocr-")
    document = fitz.open(path)
    for page_index, page in enumerate(document):
        image_path = Path(temporary.name) / f"page-{page_index}.png"
        page.get_pixmap(matrix=fitz.Matrix(2, 2), alpha=False).save(image_path)
        yield page_index, image_path
    document.close()
    temporary.cleanup()


def run_local_ocr(
    path: Path,
    *,
    ocr_factory: Callable[[], Any] | None = None,
) -> dict[str, Any]:
    """Run PaddleOCR locally and return stable, provenance-bearing output.

    ``ocr_factory`` is injectable so CI can test the adapter without downloading
    PaddleOCR models. The production default never contacts a remote OCR API.
    """
    if not path.is_file():
        raise FileNotFoundError(f"document not found: {path}")

    factory = ocr_factory or _default_factory
    try:
        engine = factory()
        inputs = _pdf_page_images(path) if path.suffix.lower() == ".pdf" else [(0, path)]
        pages: list[dict[str, Any]] = []
        for page_index, image_path in inputs:
            raw_results = list(engine.predict(str(image_path)))
            texts: list[str] = []
            confidences: list[float] = []
            for raw in raw_results:
                data = _payload_for(raw)
                texts.extend(str(value) for value in data.get("rec_texts", []))
                confidences.extend(float(value) for value in data.get("rec_scores", []))
            pages.append(
                {
                    "page_index": page_index,
                    "source_page": page_index + 1,
                    "texts": texts,
                    "confidences": confidences,
                    "text": "\n".join(texts),
                }
            )
    except OcrUnavailable:
        raise
    except Exception as exc:
        raise OcrUnavailable(f"PaddleOCR failed: {type(exc).__name__}: {exc}") from exc

    text = "\n\n".join(page["text"] for page in pages)
    return {
        "status": "paddleocr_active",
        "capability": "local-ocr",
        "engine": "PaddleOCR",
        "local_only": True,
        "path": str(path),
        "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
        "pages": pages,
        "text": text,
        "text_count": len(text),
    }


def local_ocr_status(*, ocr_factory: Callable[[], Any] | None = None) -> dict[str, Any]:
    """Report whether the configured local OCR engine can be initialized."""
    try:
        (ocr_factory or _default_factory)()
    except OcrUnavailable as exc:
        return {"capability": "local-ocr", "ready": False, "status": "unavailable", "reason": str(exc)}
    except Exception as exc:
        return {
            "capability": "local-ocr",
            "ready": False,
            "status": "unavailable",
            "reason": f"{type(exc).__name__}: {exc}",
        }
    return {"capability": "local-ocr", "ready": True, "status": "ready", "engine": "PaddleOCR", "local_only": True}
