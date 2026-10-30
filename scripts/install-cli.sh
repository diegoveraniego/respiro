#!/bin/sh
set -eu
project_dir=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
prefix=${RESPIRO_PREFIX:-"$HOME/.local"}

cargo build --release --no-default-features --features cli --bin respiro-cli --manifest-path "$project_dir/Cargo.toml"
install -Dm755 "$project_dir/target/release/respiro-cli" "$prefix/bin/.respiro-cli-install-$$"
mv -f "$prefix/bin/.respiro-cli-install-$$" "$prefix/bin/respiro-cli"
printf 'Respiro CLI instalado en %s/bin/respiro-cli\n' "$prefix"
