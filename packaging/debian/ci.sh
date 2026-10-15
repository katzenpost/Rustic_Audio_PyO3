#!/bin/sh
set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../.." && pwd)
debs=${DEBS_DIR:-/tmp/debs}
mkdir -p "$debs"
cd "$root"

packaging/debian/build.sh
cp dist/*.deb "$debs"/

apt update
apt install -y "$debs"/*.deb
packaging/debian/test.sh
packaging/debian/assert-reproducible.sh
