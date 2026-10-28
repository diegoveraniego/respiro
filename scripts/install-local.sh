#!/bin/sh
set -eu
project_dir=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
prefix=${RESPIRO_PREFIX:-"$HOME/.local"}

cargo build --release --manifest-path "$project_dir/Cargo.toml"
install -Dm755 "$project_dir/target/release/respiro" "$prefix/bin/.respiro-install-$$"
mv -f "$prefix/bin/.respiro-install-$$" "$prefix/bin/respiro"
install -Dm644 "$project_dir/data/org.diego.Respiro.desktop" "$prefix/share/applications/org.diego.Respiro.desktop"
install -Dm644 "$project_dir/data/org.diego.Respiro.metainfo.xml" "$prefix/share/metainfo/org.diego.Respiro.metainfo.xml"
install -Dm644 "$project_dir/data/icons/hicolor/scalable/apps/org.diego.Respiro.svg" "$prefix/share/icons/hicolor/scalable/apps/org.diego.Respiro.svg"

python3 - "$project_dir" "$prefix" <<'PYCODE'
from pathlib import Path
import sys
project, prefix = map(Path, sys.argv[1:])
executable = str(prefix / 'bin/respiro')
quoted = '"' + executable.replace('\\', '\\\\').replace('"', '\\"').replace('`', '\\`').replace('$', '\\$') + '"'
target = prefix / 'share/applications/org.diego.Respiro.desktop'
target.parent.mkdir(parents=True, exist_ok=True)
target.write_text('\n'.join('Exec=' + quoted if line.startswith('Exec=') else line for line in (project / 'data/org.diego.Respiro.desktop').read_text().splitlines()) + '\n')
PYCODE

if command -v gtk4-update-icon-cache >/dev/null 2>&1 && ! gtk4-update-icon-cache -f -t "$prefix/share/icons/hicolor"; then
    python3 - "$prefix" <<'PYCODE'
from pathlib import Path
import sys
prefix = Path(sys.argv[1])
launcher = prefix / 'share/applications/org.diego.Respiro.desktop'
icon = prefix / 'share/icons/hicolor/scalable/apps/org.diego.Respiro.svg'
launcher.write_text('\n'.join('Icon=' + str(icon) if line.startswith('Icon=') else line for line in launcher.read_text().splitlines()) + '\n')
PYCODE
fi
if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database "$prefix/share/applications" || true
fi
printf 'Respiro instalado en %s/bin/respiro\n' "$prefix"
