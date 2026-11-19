#!/usr/bin/env bash
# Prove the security code copied from EchoTwin was copied exactly.
#
# For every copied file: take the EchoTwin original, apply the one allowed rename
# (phonelink -> touchtwin, EchoTwin -> TouchTwin), and require the TouchTwin file to be
# identical. The only other allowed change is listed in scripts/provenance/*.diff.
#
# It also warns when the EchoTwin originals changed since they were copied (a security fix
# there may need copying here too).
#
# Usage: scripts/check-provenance.sh [path/to/EchoTwin]
set -euo pipefail
cd "$(dirname "$0")/.."

SRC="${1:-${ECHOTWIN_DIR:-$HOME/Desktop/Projects/EchoTwin/EchoTwin}}"
[[ -d "$SRC/core/src" ]] || { echo "EchoTwin not found at $SRC (pass its path)"; exit 2; }

FILES=(identity tls trust pairing discovery lan wire limiter util error probe)
rename() { sed -e 's/phonelink/touchtwin/g' -e 's/EchoTwin/TouchTwin/g' "$1"; }

fail=0
for f in "${FILES[@]}"; do
  orig="$SRC/core/src/$f.rs"
  copy="touchtwin-core/src/$f.rs"
  allowed="scripts/provenance/$f.rs.diff"
  actual="$(diff <(rename "$orig") "$copy" || true)"
  expected=""
  [[ -f "$allowed" ]] && expected="$(cat "$allowed")"
  if [[ "$actual" == "$expected" ]]; then
    echo "ok       $f.rs"
  else
    echo "MISMATCH $f.rs: differs from EchoTwin beyond the allowed rename"
    diff <(echo "$expected") <(echo "$actual") | sed 's/^/    /' || true
    fail=1
  fi
done

# Daemon files copied unchanged.
for f in netaddrs; do
  orig="$SRC/desktop/daemon/src/$f.rs"
  copy="daemon/src/$f.rs"
  if diff -q "$orig" "$copy" >/dev/null; then
    echo "ok       daemon/$f.rs"
  else
    echo "MISMATCH daemon/$f.rs: differs from EchoTwin"
    diff "$orig" "$copy" | sed 's/^/    /' || true
    fail=1
  fi
done

# Has EchoTwin changed these files since they were copied?
here="$PWD"
if ! (cd "$SRC/core/src" && sha256sum --quiet -c "$here/scripts/provenance/upstream.sha256" >/dev/null 2>&1) ||
   ! (cd "$SRC/desktop/daemon/src" && sha256sum --quiet -c "$here/scripts/provenance/upstream-daemon.sha256" >/dev/null 2>&1); then
  echo
  echo "note: EchoTwin's copies changed since $(cut -c1-12 scripts/provenance/echotwin-commit);"
  echo "      review the upstream change and copy it over if it is a fix."
fi

exit $fail
