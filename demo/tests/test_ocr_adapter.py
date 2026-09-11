from __future__ import annotations

import importlib
import sys
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).parents[1] / "mcp_server"))
from ocr import OcrUnavailable, local_ocr_status, run_local_ocr  # noqa: E402

with patch.dict(
    sys.modules,
    {
        "office": SimpleNamespace(render_to_pdf=lambda *_args, **_kwargs: {}),
        "presentation": SimpleNamespace(load_plan=lambda *_args: {}, render_plan=lambda *_args: {}),
    },
):
    server_module = importlib.import_module("server")
    call_tool = server_module.call_tool


class FakeOcr:
    def predict(self, path: str):
        return [SimpleNamespace(json=lambda: {"res": {"rec_texts": ["P-101", "PRESSURE 12 bar"], "rec_scores": [0.99, 0.91]}})]


class OcrAdapterTest(unittest.TestCase):
    def setUp(self) -> None:
        self.fixture = Path(__file__).parents[1] / "data" / "inspection_page.png"

    def test_mock_ocr_returns_page_confidence_and_provenance(self) -> None:
        result = run_local_ocr(self.fixture, ocr_factory=FakeOcr)
        self.assertEqual(result["capability"], "local-ocr")
        self.assertTrue(result["local_only"])
        self.assertEqual(result["pages"][0]["source_page"], 1)
        self.assertEqual(result["pages"][0]["confidences"], [0.99, 0.91])
        self.assertIn("P-101", result["text"])
        self.assertEqual(len(result["sha256"]), 64)

    def test_scanned_pdf_renders_pages_before_ocr(self) -> None:
        fake_pdf = self.fixture.with_suffix(".pdf")
        fake_pdf.write_bytes(b"synthetic scanned PDF")
        try:
            with patch("ocr._pdf_page_images", return_value=[(0, self.fixture), (1, self.fixture)]):
                result = run_local_ocr(fake_pdf, ocr_factory=FakeOcr)
        finally:
            fake_pdf.unlink()
        self.assertEqual([page["source_page"] for page in result["pages"]], [1, 2])

    def test_unavailable_engine_is_reported_without_remote_fallback(self) -> None:
        def unavailable():
            raise OcrUnavailable("PaddleOCR is unavailable")

        status = local_ocr_status(ocr_factory=unavailable)
        self.assertFalse(status["ready"])
        with self.assertRaises(OcrUnavailable):
            run_local_ocr(self.fixture, ocr_factory=unavailable)

    def test_mcp_tool_uses_adapter_and_returns_structured_content(self) -> None:
        with patch.object(server_module, "run_local_ocr", return_value={"capability": "local-ocr", "pages": [], "text": "", "text_count": 0, "local_only": True}) as mocked:
            result = call_tool("ocr_document", {"path": "data/inspection_page.png"})
        mocked.assert_called_once()
        self.assertEqual(result["structuredContent"]["capability"], "local-ocr")

    def test_path_scope_is_enforced_before_ocr(self) -> None:
        with self.assertRaises(ValueError):
            call_tool("ocr_document", {"path": "/etc/passwd"})


if __name__ == "__main__":
    unittest.main()
