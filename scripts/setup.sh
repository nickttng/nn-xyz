#!/usr/bin/env bash
# One-time setup after cloning: turn on the repo's git hooks.
set -euo pipefail
git config core.hooksPath .githooks
echo "Git hooks enabled (.githooks/). Requires: gitleaks, exiftool."
