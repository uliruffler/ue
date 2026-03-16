#!/usr/bin/env bash
# Creates the homebrew-ue tap repository locally.
# After running this, push the generated directory to github.com/uliruffler/homebrew-ue
# so users can install with:
#   brew tap uliruffler/ue && brew install ue
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TAP_DIR="${1:-$HOME/homebrew-ue}"

echo "Creating tap repository at: $TAP_DIR"
mkdir -p "$TAP_DIR/Formula"

# Copy formula from project
cp "$REPO_ROOT/HomebrewFormula/ue.rb" "$TAP_DIR/Formula/ue.rb"

# Initialize git if not already a repo
if [[ ! -d "$TAP_DIR/.git" ]]; then
  git -C "$TAP_DIR" init
  git -C "$TAP_DIR" add .
  git -C "$TAP_DIR" commit -m "Initial Homebrew tap for ue"
fi

cat <<EOF

Tap repo created at: $TAP_DIR

Next steps:
  1. Create an empty GitHub repository named 'homebrew-ue' under:
       https://github.com/uliruffler/homebrew-ue
  2. Push this directory to that repository:
       cd "$TAP_DIR"
       git remote add origin git@github.com:uliruffler/homebrew-ue.git
       git push -u origin main
  3. Update the SHA256 in Formula/ue.rb after creating the v0.0.1 release:
       $REPO_ROOT/scripts/update-formula-sha.sh 0.0.1

Users can then install with:
  brew tap uliruffler/ue
  brew install ue

To install the latest development build directly:
  brew install --HEAD uliruffler/ue/ue
EOF
