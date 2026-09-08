"""Safe headless office-suite rendering for local artifact verification."""
from __future__ import annotations

import shutil
import subprocess
import tempfile
from pathlib import Path
from typing import Any


def render_to_pdf(path: Path, *, timeout: int = 60) -> dict[str, Any]:
    """Convert a local office document to a temporary PDF without shell execution."""
    executable = shutil.which("libreoffice") or shutil.which("soffice")
    result: dict[str, Any] = {
        "available": executable is not None,
        "attempted": False,
        "rendered": False,
        "format": "pdf",
    }
    if executable is None:
        result["diagnostic"] = "LibreOffice executable not found"
        return result

    result["attempted"] = True
    with tempfile.TemporaryDirectory(prefix="crudo-office-") as temp_dir:
        output_dir = Path(temp_dir) / "output"
        profile_dir = Path(temp_dir) / "profile"
        output_dir.mkdir()
        profile_dir.mkdir()
        command = [
            executable,
            "--headless",
            f"-env:UserInstallation={profile_dir.as_uri()}",
            "--convert-to",
            "pdf",
            "--outdir",
            str(output_dir),
            str(path),
        ]
        try:
            completed = subprocess.run(
                command,
                check=False,
                capture_output=True,
                text=True,
                timeout=timeout,
            )
        except subprocess.TimeoutExpired:
            result["diagnostic"] = f"LibreOffice conversion timed out after {timeout}s"
            return result
        except OSError as exc:
            result["diagnostic"] = f"LibreOffice could not start: {type(exc).__name__}: {exc}"
            return result

        pdf_path = output_dir / f"{path.stem}.pdf"
        result["return_code"] = completed.returncode
        result["diagnostic"] = (completed.stderr or completed.stdout or "").strip()[-1000:]
        if completed.returncode == 0 and pdf_path.is_file() and pdf_path.stat().st_size > 0:
            result["rendered"] = True
            result["pdf_bytes"] = pdf_path.stat().st_size
        elif not result["diagnostic"]:
            result["diagnostic"] = "LibreOffice did not produce a non-empty PDF"
    return result
