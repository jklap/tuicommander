"""Materialise spike host registration only inside an explicit throwaway profile."""
import hashlib
import json
from pathlib import Path
import sys

root = Path(__file__).resolve().parent
profile = Path(sys.argv[1]).expanduser().resolve()
allowed = (Path.home() / 'Gits/.tmp/dm2-s0').resolve()
if not profile.is_relative_to(allowed):
    raise SystemExit('Refusing a profile outside ~/Gits/.tmp/dm2-s0')
# Chromium unpacked IDs are the first 128 bits of SHA256 of the absolute path.
digest = hashlib.sha256(str(root / 'extension').encode()).hexdigest()[:32]
extension_id = ''.join(chr(ord('a') + int(c, 16)) for c in digest)
folder = profile / 'NativeMessagingHosts'
folder.mkdir(parents=True, exist_ok=True)
(folder / 'dev.tuic.dm2_s0.json').write_text(json.dumps({
    'name': 'dev.tuic.dm2_s0', 'description': 'Throwaway S0 echo, no TUIC state',
    'path': str(root / 'host.py'), 'type': 'stdio',
    'allowed_origins': [f'chrome-extension://{extension_id}/'],
}, indent=2) + '\n')
print(json.dumps({'profile': str(profile), 'extension_id': extension_id}))
