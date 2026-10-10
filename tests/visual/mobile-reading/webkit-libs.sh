#!/usr/bin/env bash
# Extract optional host prerequisites inside the workspace; never install system packages.
set -euo pipefail
lib_root=/home/rb/Gits/.tmp/pw/mobile-reading-libs
mkdir -p "$lib_root/packages" "$lib_root/root"
cd "$lib_root/packages"
apt-get download libevent-2.1-7t64 libgstreamer-plugins-bad1.0-0 libflite1 libavif16 gstreamer1.0-libav libdav1d7 libgav1-1 librav1e0 libsvtav1enc1d1 libyuv0 libx264-164 libx264-dev libabsl20220623t64
for archive in ./*.deb; do dpkg-deb -x "$archive" "$lib_root/root"; done

# The development package uses an absolute symlink; keep this extracted copy local.
ln -sf libx264.so.164 "$lib_root/root/usr/lib/aarch64-linux-gnu/libx264.so"
