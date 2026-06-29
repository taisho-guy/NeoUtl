#!/usr/bin/env bash
set -e

PROFILE=${1:-debug}
CARGO_FLAG=$([ "$PROFILE" = "debug" ] && echo "dev" || echo "$PROFILE")
EXT="so"
[[ "$OSTYPE" == "darwin"* ]] && EXT="dylib"
[[ "$OSTYPE" == "msys"* || "$OSTYPE" == "win32" ]] && EXT="dll"

PLUGINS=(
    "crates/objects/tetrahedron libneoutl_object_tetrahedron"
    "crates/objects/cube        libneoutl_object_cube"
    "crates/objects/text        libneoutl_object_text"
)

mkdir -p "target/${PROFILE}/objects"

for entry in "${PLUGINS[@]}"; do
    path=$(echo "$entry" | awk '{print $1}')
    lib=$(echo "$entry"  | awk '{print $2}')
    cargo build --profile "$CARGO_FLAG" --manifest-path "${path}/Cargo.toml"
    cp "target/${PROFILE}/deps/${lib}.${EXT}" "target/${PROFILE}/objects/"
done
