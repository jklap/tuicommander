#!/usr/bin/env bash
# Isolated equivalent of the bundled WPE launcher, with workspace-only host libraries.
set -euo pipefail
browser_root="$PLAYWRIGHT_BROWSERS_PATH/webkit-2370/minibrowser-wpe"
local_root=/home/rb/Gits/.tmp/pw/mobile-reading-libs/root
export WEBKIT_EXEC_PATH="$browser_root/bin"
export WEBKIT_INJECTED_BUNDLE_PATH="$browser_root/lib"
export WEBKIT_INSPECTOR_RESOURCES_PATH="$browser_root/share"
export WEBKIT_FORCE_COMPLEX_TEXT=1
export LD_LIBRARY_PATH="$local_root/usr/lib/aarch64-linux-gnu:$local_root/lib/aarch64-linux-gnu:$browser_root/lib:$browser_root/sys/lib"
exec "$browser_root/bin/MiniBrowser" "$@"
