#!/usr/bin/env python3
from __future__ import annotations

from pathlib import Path
import re
import sys

ROOT = Path(__file__).resolve().parents[2]
FILES = [
    ROOT / 'README.md',
    ROOT / 'USAGE.md',
    ROOT / 'PARITY.md',
    ROOT / 'PHILOSOPHY.md',
    ROOT / 'ROADMAP.md',
    ROOT / '.github' / 'FUNDING.yml',
]
FILES.extend(sorted((ROOT / 'docs').rglob('*.md')) if (ROOT / 'docs').exists() else [])

FORBIDDEN = {
    r'github\.com/Yeachan-Heo/crudo(?:[/?#)\s]|$)': 'replace old crudo GitHub links with lu2gi2/crudo',
    r'github\.com/code-yeongyu/crudo': 'replace stale alternate crudo GitHub links with lu2gi2/crudo',
    r'github\.com/ultraworkers/crudo': 'replace stale ultraworkers crudo GitHub links with lu2gi2/crudo',
    r'discord\.gg/6ztZB9jvWq': 'remove the stale UltraWorkers Discord invite',
    r'api\.star-history\.com/svg\?repos=Yeachan-Heo/crudo': 'update star-history embeds to lu2gi2/crudo',
    r'api\.star-history\.com/svg\?repos=ultraworkers/crudo': 'update star-history embeds to lu2gi2/crudo',
    r'star-history\.com/#Yeachan-Heo/crudo': 'update star-history links to lu2gi2/crudo',
    r'star-history\.com/#ultraworkers/crudo': 'update star-history links to lu2gi2/crudo',
    r'assets/instructkr\.png': 'remove stale instructkr image references',
}

errors: list[str] = []
for path in FILES:
    if not path.exists():
        continue
    text = path.read_text(encoding='utf-8')
    for pattern, message in FORBIDDEN.items():
        for match in re.finditer(pattern, text):
            line = text.count('\n', 0, match.start()) + 1
            errors.append(f'{path.relative_to(ROOT)}:{line}: {message}')

if errors:
    print('doc source-of-truth check failed:', file=sys.stderr)
    for error in errors:
        print(f'  - {error}', file=sys.stderr)
    sys.exit(1)

print('doc source-of-truth check passed')
