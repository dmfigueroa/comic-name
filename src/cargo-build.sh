#!/bin/sh
set -eu

cargo_bin=$1
manifest=$2
target_dir=$3
profile=$4
output=$5

if [ "$profile" = release ]; then
  "$cargo_bin" build --manifest-path "$manifest" --target-dir "$target_dir" --release
else
  "$cargo_bin" build --manifest-path "$manifest" --target-dir "$target_dir"
fi

cp "$target_dir/$profile/comic-name" "$output"
