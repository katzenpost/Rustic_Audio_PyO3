#!/bin/sh
set -eu

cd "$(dirname "$0")/../.."
dpkg-buildpackage -b -uc -us
mkdir -p dist
mv ../python3-rustic-audio-tool_*.deb dist/
sha256sum dist/*.deb
