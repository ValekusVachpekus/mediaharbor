#!/usr/bin/env python3
import base64
import hashlib
from pathlib import Path
key = (Path(__file__).resolve().parent.parent / 'apps/extension/public/chromium-key.txt').read_text().strip()
digest = hashlib.sha256(base64.b64decode(key)).hexdigest()[:32]
print(''.join(chr(ord('a') + int(c, 16)) for c in digest))
