#!/usr/bin/env bash
# One-time setup: activate git hooks from .githooks/
set -euo pipefail
git config core.hooksPath .githooks
echo "Git hooks activated from .githooks/ (pre-push: clippy + tests)"
