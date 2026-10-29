#!/usr/bin/env python3
"""Remove only MediaHarbor's per-user browser registrations; preserve files/jobs."""
import os
from pathlib import Path
import sys

if sys.platform == 'win32':
    import winreg
    for vendor in ['Google\\Chrome', 'Microsoft\\Edge', 'Mozilla']:
        try:
            winreg.DeleteKey(winreg.HKEY_CURRENT_USER, f'Software\\{vendor}\\NativeMessagingHosts\\io.mediaharbor.host')
        except FileNotFoundError:
            pass
else:
    home = Path.home()
    config = Path(os.environ.get('XDG_CONFIG_HOME', home / '.config'))
    for directory in [config / 'google-chrome/NativeMessagingHosts', config / 'chromium/NativeMessagingHosts', config / 'microsoft-edge/NativeMessagingHosts', home / '.mozilla/native-messaging-hosts']:
        (directory / 'io.mediaharbor.host.json').unlink(missing_ok=True)
