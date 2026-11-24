#!/bin/sh
# Regenerates include/meros_picture_tools.h (and the Swift package's copy)
# from the C ABI in crates/picture-tools/src/capi.rs. Needs cbindgen
# (`cargo install cbindgen`). With --verify, fails if they're stale.
set -e
cd "$(dirname "$0")/.."
out=$(mktemp)
cbindgen --quiet --config crates/picture-tools/cbindgen.toml --output "$out" crates/picture-tools
# The C ABI is the `capi` feature; the header is only that, unguarded.
grep -v -x -e '#if defined(MPT_CAPI_ALWAYS)' -e '#endif' "$out" | cat -s > "$out.h"
rm -f "$out"
for target in include/meros_picture_tools.h swift/Sources/CMerosPictureTools/include/meros_picture_tools.h; do
  if [ "$1" = "--verify" ]; then
    cmp -s "$out.h" "$target" || { echo "$target is stale: run scripts/gen-header.sh"; rm -f "$out.h"; exit 1; }
  else
    mkdir -p "$(dirname "$target")"
    cp "$out.h" "$target"
  fi
done
rm -f "$out.h"
