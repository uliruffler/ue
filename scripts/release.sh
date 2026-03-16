#!/usr/bin/env bash
# Full release workflow for ue.
# Usage: scripts/release.sh <version>
# Example: scripts/release.sh 0.1.0
#
# Environment variables:
#   UE_TAP_DIR   Path to the local homebrew-ue tap repository (default: ~/homebrew-ue)
#   SKIP_BUILD   Set to 1 to skip the cargo build verification step
#
# Prerequisites: git, cargo, curl, sha256sum or shasum
# Optional: gh (GitHub CLI) for automated release creation
set -euo pipefail

# ── Args & config ────────────────────────────────────────────────────────────

VERSION="${1:?Usage: $0 <version>}"
VERSION="${VERSION#v}"   # strip leading 'v' if provided

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
FORMULA="$REPO_ROOT/HomebrewFormula/ue.rb"
TAP_DIR="${UE_TAP_DIR:-$HOME/homebrew-ue}"
TARBALL_URL="https://github.com/uliruffler/ue/archive/refs/tags/v${VERSION}.tar.gz"

# ── Helpers ──────────────────────────────────────────────────────────────────

step() { echo; echo "▶ $*"; }
die()  { echo "✗ $*" >&2; exit 1; }
ok()   { echo "✓ $*"; }

# ── Pre-flight checks ────────────────────────────────────────────────────────

step "Pre-flight checks"

[[ -f "$REPO_ROOT/Cargo.toml" ]] || die "Not a ue repo (Cargo.toml not found)"

CURRENT_VERSION="$(grep '^version = ' "$REPO_ROOT/Cargo.toml" | head -1 | sed 's/version = "\(.*\)"/\1/')"
if [[ "$CURRENT_VERSION" == "$VERSION" ]]; then
  echo "  Cargo.toml already at v${VERSION} — skipping version bump"
  BUMP_VERSION=0
else
  BUMP_VERSION=1
fi

HAS_GH=0
if command -v gh &>/dev/null; then
  HAS_GH=1
  ok "GitHub CLI (gh) detected — will create release automatically"
else
  echo "  gh not found — you will need to create the GitHub release manually"
fi

TAP_EXISTS=0
if [[ -d "$TAP_DIR/.git" ]]; then
  TAP_EXISTS=1
  ok "Tap repo found at $TAP_DIR"
else
  echo "  Tap repo not found at $TAP_DIR — formula will be updated locally only"
  echo "  Run scripts/create-homebrew-tap.sh first to set up the tap"
fi

# Check for uncommitted changes (other than what we're about to do)
cd "$REPO_ROOT"
if [[ -n "$(git status --porcelain)" ]]; then
  die "Working directory has uncommitted changes. Commit or stash them first."
fi

# ── Step 1: Bump version in Cargo.toml ───────────────────────────────────────

if [[ "$BUMP_VERSION" -eq 1 ]]; then
  step "Bumping Cargo.toml: $CURRENT_VERSION → $VERSION"
  # macOS sed needs an explicit backup extension; Linux sed is fine with -i alone
  if sed --version 2>&1 | grep -q GNU; then
    sed -i "s/^version = \"${CURRENT_VERSION}\"/version = \"${VERSION}\"/" "$REPO_ROOT/Cargo.toml"
  else
    sed -i '' "s/^version = \"${CURRENT_VERSION}\"/version = \"${VERSION}\"/" "$REPO_ROOT/Cargo.toml"
  fi
  ok "Cargo.toml updated"
fi

# ── Step 2: Build & test ──────────────────────────────────────────────────────

if [[ "${SKIP_BUILD:-0}" -ne 1 ]]; then
  step "Building release binary (verifies compilation + updates Cargo.lock)"
  cargo build --release --manifest-path "$REPO_ROOT/Cargo.toml"
  ok "Build succeeded"
else
  echo "  SKIP_BUILD set — skipping cargo build"
fi

# ── Step 3: Commit, tag, push ─────────────────────────────────────────────────

step "Committing and tagging v${VERSION}"
cd "$REPO_ROOT"
git add Cargo.toml Cargo.lock
git commit -m "Release v${VERSION}"
git tag "v${VERSION}"
git push origin main
git push origin "v${VERSION}"
ok "Pushed commit and tag v${VERSION}"

# ── Step 4: Create GitHub Release ────────────────────────────────────────────

if [[ "$HAS_GH" -eq 1 ]]; then
  step "Creating GitHub Release v${VERSION}"
  gh release create "v${VERSION}" \
    --repo uliruffler/ue \
    --title "v${VERSION}" \
    --generate-notes
  ok "GitHub Release created"
else
  echo
  echo "  ╔══════════════════════════════════════════════════════════════════╗"
  echo "  ║  ACTION REQUIRED: Create the GitHub Release manually            ║"
  echo "  ║  https://github.com/uliruffler/ue/releases/new?tag=v${VERSION}  ║"
  echo "  ║  Publish it, then press Enter to continue...                    ║"
  echo "  ╚══════════════════════════════════════════════════════════════════╝"
  read -r
fi

# ── Step 5: Wait for tarball to become available, then compute SHA256 ────────

step "Waiting for release tarball to become available on GitHub"
MAX_ATTEMPTS=20
WAIT_SECONDS=10
TMPFILE="$(mktemp /tmp/ue-release-XXXXXX.tar.gz)"

for attempt in $(seq 1 "$MAX_ATTEMPTS"); do
  echo "  Attempt $attempt/$MAX_ATTEMPTS: downloading $TARBALL_URL ..."
  if curl -fsSL --max-time 30 "$TARBALL_URL" -o "$TMPFILE" 2>/dev/null; then
    ok "Tarball downloaded"
    break
  fi
  if [[ "$attempt" -eq "$MAX_ATTEMPTS" ]]; then
    rm -f "$TMPFILE"
    die "Tarball not available after $((MAX_ATTEMPTS * WAIT_SECONDS))s. Check the GitHub release and re-run update-formula-sha.sh manually."
  fi
  echo "  Not yet available, waiting ${WAIT_SECONDS}s..."
  sleep "$WAIT_SECONDS"
done

if command -v sha256sum &>/dev/null; then
  SHA256="$(sha256sum "$TMPFILE" | awk '{print $1}')"
else
  SHA256="$(shasum -a 256 "$TMPFILE" | awk '{print $1}')"
fi
rm -f "$TMPFILE"
ok "SHA256: $SHA256"

# ── Step 6: Update Homebrew formula ──────────────────────────────────────────

step "Updating Homebrew formula"
if sed --version 2>&1 | grep -q GNU; then
  sed -i \
    -e "s|url \"https://github.com/uliruffler/ue/archive/refs/tags/v[^\"]*\"|url \"$TARBALL_URL\"|" \
    -e "s|sha256 \"[a-f0-9]*\"|sha256 \"$SHA256\"|" \
    "$FORMULA"
else
  sed -i '' \
    -e "s|url \"https://github.com/uliruffler/ue/archive/refs/tags/v[^\"]*\"|url \"$TARBALL_URL\"|" \
    -e "s|sha256 \"[a-f0-9]*\"|sha256 \"$SHA256\"|" \
    "$FORMULA"
fi
ok "Formula updated: $FORMULA"

# ── Step 7: Push updated formula to tap repo ─────────────────────────────────

if [[ "$TAP_EXISTS" -eq 1 ]]; then
  step "Pushing updated formula to tap repo ($TAP_DIR)"
  cp "$FORMULA" "$TAP_DIR/Formula/ue.rb"
  cd "$TAP_DIR"
  git add Formula/ue.rb
  git commit -m "ue ${VERSION}"
  git push
  ok "Tap repo updated and pushed"
else
  echo
  echo "  Tap repo not set up. Copy the formula manually:"
  echo "    cp $FORMULA <your-homebrew-ue-repo>/Formula/ue.rb"
fi

# ── Done ──────────────────────────────────────────────────────────────────────

echo
echo "╔══════════════════════════════════════════════╗"
echo "║  Released ue v${VERSION} successfully!       "
echo "╚══════════════════════════════════════════════╝"
echo
echo "Users can install with:"
echo "  brew tap uliruffler/ue && brew install ue"
echo "  brew upgrade ue   # for existing installs"
