#!/bin/sh
set -eu

script=$(CDPATH= cd -- "$(dirname "$0")" && pwd)/check-version.sh
root=$(mktemp -d "${TMPDIR:-/var/tmp}/check-version-test.XXXXXX")
trap 'rm -rf "$root"' EXIT
failed=0
n=0

tree() {
    n=$((n + 1))
    dir=$root/$n
    mkdir -p "$dir/packaging/debian" "$dir/debian"
    cp "$script" "$dir/packaging/debian/check-version.sh"
    printf '[package]\nname = "rustic_audio_tool"\nversion = "%s"\n' "$1" \
        > "$dir/Cargo.toml"
    printf 'name = "rustic_audio_tool"\nversion = "%s"\n' "$2" \
        > "$dir/Cargo.lock"
    printf 'rustic-audio-tool (%s) unstable; urgency=medium\n' "$3" \
        > "$dir/debian/changelog"
}

expect() {
    want=$1
    name=$2
    shift 2
    if "$dir/packaging/debian/check-version.sh" "$@" >/dev/null 2>&1; then
        got=0
    else
        got=1
    fi
    if [ "$got" = "$want" ]; then
        printf 'ok   %s\n' "$name"
    else
        printf 'FAIL %s: wanted %s, got %s\n' "$name" "$want" "$got"
        failed=1
    fi
}

tree 0.0.3 0.0.3 0.0.3
expect 0 "three agreeing versions pass"
expect 0 "and pass against their own tag" v0.0.3
expect 1 "but fail against another tag" v0.0.4
expect 1 "and fail a tag with no v" 0.0.3

tree 0.0.3 0.0.2 0.0.3
expect 1 "a stale Cargo.lock fails"

tree 0.0.3 0.0.3 0.0.2
expect 1 "a stale changelog fails"

tree 0.0.3 0.0.3 0.0.3
rm "$dir/debian/changelog"
expect 1 "a missing changelog fails"

exit "$failed"
