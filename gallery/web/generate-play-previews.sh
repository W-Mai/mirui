#!/bin/sh
set -eu

web_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repo_root=$(CDPATH= cd -- "$web_dir/../.." && pwd)
output_parent="$repo_root/target/gallery-site"
output_dir="$output_parent/play-previews"
stamp="$output_parent/play-previews.stamp"

if [ -f "$stamp" ] && [ -d "$output_dir" ]; then
    if ! find \
        "$repo_root/src" \
        "$repo_root/gallery/src" \
        "$repo_root/gallery/examples/tools" \
        "$repo_root/Cargo.toml" \
        "$repo_root/Cargo.lock" \
        "$repo_root/gallery/Cargo.toml" \
        -type f -newer "$stamp" -print -quit | grep -q .; then
        exit 0
    fi
fi

mkdir -p "$output_parent"
temporary_dir=$(mktemp -d "$output_parent/.play-previews.XXXXXX")
trap 'rm -rf "$temporary_dir"' EXIT HUP INT TERM

cargo run \
    --quiet \
    --manifest-path "$repo_root/Cargo.toml" \
    --package gallery \
    --no-default-features \
    --features snapshot \
    --example play_previews_snapshot \
    --release \
    -- "$temporary_dir"

for preview in \
    marble_play twin_beacons tidal_atlas echo_walker pixel_loom lumen_lab folding_ark \
    atlas_restoration module_factory orbital_mission logic_circuit moss_study pocket_post
do
    test -s "$temporary_dir/$preview.png"
done

rm -rf "$output_dir"
mv "$temporary_dir" "$output_dir"
touch "$stamp"
