from __future__ import annotations

import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from office import render_to_pdf


class OfficeRenderingTest(unittest.TestCase):
    def test_existing_presentation_renders_without_output_artifact(self) -> None:
        source = Path(__file__).parents[1] / "output" / "truck-count-board-deck.pptx"
        result = render_to_pdf(source)
        self.assertTrue(result["available"])
        self.assertTrue(result["rendered"], result)
        self.assertGreater(result["pdf_bytes"], 0)
        self.assertFalse((source.with_suffix(".pdf")).exists())

    def test_missing_office_is_reported_truthfully(self) -> None:
        source = Path(__file__).parents[1] / "output" / "truck-count-board-deck.pptx"
        with patch("office.shutil.which", return_value=None):
            result = render_to_pdf(source)
        self.assertEqual(result["available"], False)
        self.assertEqual(result["attempted"], False)
        self.assertEqual(result["rendered"], False)
        self.assertIn("not found", result["diagnostic"])


if __name__ == "__main__":
    unittest.main()
