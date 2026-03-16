#!/usr/bin/env bash
# Updates the url + sha256 fields in the Homebrew formula for a new release.
# Usage: scripts/update-formula-sha.sh <version>
# Example: scripts/update-formula-sha.sh 0.1.0
#
# Prerequisites: curl, sha256sum (Linux) or shasum (macOS)
set -euo pipefail

VERSION="${1:?Usage: $0 <version>}"
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
FORMULA="$REPO_ROOT/HomebrewFormula/ue.rb"

TARBALL_URL="https://github.com/uliruffler/ue/archive/refs/tags/v${VERSION}.tar.gz"
TMPFILE="$(mktemp /tmp/ue-release-XXXXXX.tar.gz)"

echo "Downloading $TARBALL_URL ..."
curl -fsSL "$TARBALL_URL" -o "$TMPFILE"

if command -v sha256sum &>/dev/null; then
  SHA256="$(sha256sum "$TMPFILE" | awk '{print $1}')"
else
  SHA256="$(shasum -a 256 "$TMPFILE" | awk '{print $1}')"
fi
rm -f "$TMPFILE"

echo "SHA256: $SHA256"

# Replace url and sha256 in the formula
sed -i \
  -e "s|url \"https://github.com/uliruffler/ue/archive/refs/tags/v[^\"]*\"|url \"$TARBALL_URL\"|" \
  -e "s|sha256 \"[a-f0-9]*\"|sha256 \"$SHA256\"|" \
  "$FORMULA"

# Update the version comment in Cargo.toml if desired (optional)
echo "Updated $FORMULA for v${VERSION}"
echo "  url    → $TARBALL_URL"
echo "  sha256 → $SHA256"
echo ""
echo "Commit and push the updated formula to your homebrew-ue tap."
