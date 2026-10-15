#!/bin/sh
set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../.." && pwd)
tag=${1-}

die() {
    printf '%s\n' "$*" >&2
    exit 1
}

cargo=$(sed -n '/^\[package\]/,/^\[/s/^version = "\([0-9][0-9.]*\)"$/\1/p' \
    "$root/Cargo.toml" | head -n 1)
test -n "$cargo" || die "Cargo.toml: no package version"

lock=$(sed -n '/^name = "rustic_audio_tool"$/{n;s/^version = "\([0-9][0-9.]*\)"$/\1/p;}' \
    "$root/Cargo.lock" | head -n 1)
test -n "$lock" || die "Cargo.lock: no rustic_audio_tool version"

changelog=$(sed -n '1s/^[^ ]* (\([^)]*\)).*/\1/p' "$root/debian/changelog")
test -n "$changelog" || die "debian/changelog: no version on line 1"

test "$lock" = "$cargo" \
    || die "Cargo.lock $lock != Cargo.toml $cargo"
test "$changelog" = "$cargo" \
    || die "debian/changelog $changelog != Cargo.toml $cargo"

if [ -n "$tag" ]; then
    case $tag in
    v*) ;;
    *) die "tag $tag does not start with v" ;;
    esac
    test "$cargo" = "${tag#v}" \
        || die "version $cargo does not match tag $tag"
fi
