#!/usr/bin/env bash
# scripts/release.sh — Cut a new statico release.
#
# Usage:
#   ./scripts/release.sh <version>                 # bump → test → commit → tag (local only)
#   ./scripts/release.sh <version> --push          # also push the commit + tag
#   ./scripts/release.sh <version> --dry-run       # print what would change, don't write
#   ./scripts/release.sh <version> --skip-tests    # skip `cargo test` (emergency)
#   ./scripts/release.sh <version> --allow-dirty   # allow uncommitted changes
#
# The version argument may include or omit the leading `v`.
#
# What the script does:
#   1. Validate semver, clean tree, branch
#   2. Update [package] version in Cargo.toml
#   3. Update top-level version in npm/package.json (and statico.rb cask if present)
#   4. Refresh Cargo.lock so the new version is recorded
#   5. Move CHANGELOG.md `## [Unreleased]` entries under a new `## [X.Y.Z] - DATE` section
#   6. Run cargo test, cargo clippy --all-targets -- -D warnings, cargo build --release
#   7. Commit "chore: release vX.Y.Z"
#   8. Create annotated tag vX.Y.Z
#   9. Tell you how to push (or push directly with --push)

set -euo pipefail

# ── Pretty print helpers ────────────────────────────────────────────────────
if [ -t 1 ]; then
  C_RED='\033[31m'; C_GRN='\033[32m'; C_YLW='\033[33m'; C_BLU='\033[34m'; C_DIM='\033[2m'; C_RST='\033[0m'
else
  C_RED=''; C_GRN=''; C_YLW=''; C_BLU=''; C_DIM=''; C_RST=''
fi
say()   { printf "%b▶ %s%b\n" "$C_BLU" "$1" "$C_RST"; }
ok()    { printf "%b✓ %s%b\n" "$C_GRN" "$1" "$C_RST"; }
warn()  { printf "%b! %s%b\n" "$C_YLW" "$1" "$C_RST" >&2; }
die()   { printf "%bx %s%b\n" "$C_RED" "$1" "$C_RST" >&2; exit 1; }
dim()   { printf "%b%s%b\n" "$C_DIM" "$1" "$C_RST"; }

# ── Args ────────────────────────────────────────────────────────────────────
VERSION=""
PUSH=false
DRY_RUN=false
SKIP_TESTS=false
ALLOW_DIRTY=false

while [ $# -gt 0 ]; do
  case "$1" in
    --push)         PUSH=true ;;
    --dry-run)      DRY_RUN=true ;;
    --skip-tests)   SKIP_TESTS=true ;;
    --allow-dirty)  ALLOW_DIRTY=true ;;
    -h|--help)
      sed -n '1,/^set/p' "$0" | sed 's/^# \{0,1\}//;1d;$d'
      exit 0
      ;;
    -*) die "unknown flag: $1" ;;
    *)
      [ -z "$VERSION" ] || die "version specified twice: '$VERSION' and '$1'"
      VERSION="$1"
      ;;
  esac
  shift
done

[ -n "$VERSION" ] || die "missing version. Usage: $0 <version> [--push] [--dry-run]"

# Strip leading 'v' if the user typed one.
VERSION="${VERSION#v}"

# Strict semver: MAJOR.MINOR.PATCH(-pre)?(+build)?
if ! printf '%s' "$VERSION" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?(\+[0-9A-Za-z.-]+)?$'; then
  die "version '$VERSION' is not a valid semver (expected X.Y.Z[-pre][+build])"
fi
TAG="v$VERSION"
DATE="$(date -u +%Y-%m-%d)"

# ── Locate the repo root ────────────────────────────────────────────────────
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
say "Releasing from $ROOT"
dim "  version: $VERSION"
dim "  tag:     $TAG"
dim "  date:    $DATE"
$DRY_RUN && warn "DRY-RUN: no files will be modified, no commits will be made"

# ── Preflight ───────────────────────────────────────────────────────────────
say "Preflight checks"

if ! command -v git >/dev/null; then die "git not found"; fi
if ! command -v cargo >/dev/null; then die "cargo not found"; fi
if ! command -v node >/dev/null; then warn "node not found — npm/package.json will be skipped"; fi

if [ ! -f Cargo.toml ]; then die "Cargo.toml not found in $ROOT"; fi

if ! $ALLOW_DIRTY && [ -n "$(git status --porcelain)" ]; then
  git status --short
  die "working tree not clean (use --allow-dirty to override)"
fi

CURRENT_BRANCH="$(git branch --show-current)"
case "$CURRENT_BRANCH" in
  main|master|release/*|audit/*) ;;
  *) warn "on branch '$CURRENT_BRANCH' (release is usually cut from main)" ;;
esac

if git rev-parse "$TAG" >/dev/null 2>&1; then
  die "tag $TAG already exists locally"
fi
if git ls-remote --tags origin "refs/tags/$TAG" 2>/dev/null | grep -q "$TAG"; then
  die "tag $TAG already exists on origin"
fi

ok "preflight passed"

# ── Helpers for atomic file rewrites ────────────────────────────────────────
write_file() {
  local target="$1"
  local content="$2"
  if $DRY_RUN; then
    dim "  would update $target"
    return 0
  fi
  printf '%s' "$content" >"$target.tmp"
  mv "$target.tmp" "$target"
}

# ── Cargo.toml version bump (only [package] section) ────────────────────────
say "Updating Cargo.toml"
NEW_CARGO="$(awk -v v="$VERSION" '
  BEGIN { in_pkg = 0; replaced = 0 }
  /^\[package\][[:space:]]*$/   { in_pkg = 1; print; next }
  /^\[/                          { in_pkg = 0 }
  in_pkg && /^version[[:space:]]*=/ && !replaced {
    print "version = \"" v "\""
    replaced = 1
    next
  }
  { print }
  END { if (!replaced) { print "// MISSING: no [package] version line" > "/dev/stderr"; exit 1 } }
' Cargo.toml)" || die "failed to bump Cargo.toml"
write_file Cargo.toml "$NEW_CARGO"
ok "Cargo.toml [package] version → $VERSION"

# ── npm/package.json version bump ───────────────────────────────────────────
if [ -f npm/package.json ] && command -v node >/dev/null; then
  say "Updating npm/package.json"
  if $DRY_RUN; then
    dim "  would set npm/package.json version → $VERSION"
  else
    node -e "
      const fs = require('fs');
      const p = 'npm/package.json';
      const j = JSON.parse(fs.readFileSync(p, 'utf8'));
      j.version = process.argv[1];
      fs.writeFileSync(p, JSON.stringify(j, null, 2) + '\n');
    " "$VERSION"
  fi
  ok "npm/package.json version → $VERSION"
fi

# ── install/statico.rb (Homebrew cask) version bump, if present ─────────────
if [ -f install/statico.rb ]; then
  say "Updating install/statico.rb"
  # Match `/v<digits-and-dots-with-optional-suffix>/` in the asset URL.
  # `-` is placed at the end of the character class to avoid BSD-sed range issues.
  NEW_BREW="$(sed -E "s#/v[0-9]+\.[0-9]+\.[0-9]+([0-9A-Za-z.+-]*)/#/v$VERSION/#g" install/statico.rb)"
  write_file install/statico.rb "$NEW_BREW"
  ok "install/statico.rb URL → v$VERSION"
fi

# ── CHANGELOG.md: insert new section after [Unreleased] ─────────────────────
if [ -f CHANGELOG.md ]; then
  if ! grep -q '^## \[Unreleased\]' CHANGELOG.md; then
    warn "CHANGELOG.md has no '## [Unreleased]' heading — skipping"
  else
    say "Updating CHANGELOG.md"
    NEW_CHANGELOG="$(awk -v v="$VERSION" -v d="$DATE" '
      BEGIN { replaced = 0 }
      !replaced && /^## \[Unreleased\][[:space:]]*$/ {
        print
        print ""
        print "## [" v "] - " d
        replaced = 1
        next
      }
      { print }
    ' CHANGELOG.md)"
    write_file CHANGELOG.md "$NEW_CHANGELOG"
    ok "CHANGELOG.md split: [Unreleased] → [$VERSION] - $DATE"
  fi
fi

# ── .github/release-notes.md: extract version section from CHANGELOG ───────
RELEASE_NOTES_TEMPLATE=".github/release-notes-template.md"
RELEASE_NOTES_FILE=".github/release-notes.md"
if [ -f "$RELEASE_NOTES_TEMPLATE" ] && [ -f CHANGELOG.md ]; then
  say "Generating $RELEASE_NOTES_FILE"
  # Extract everything between ## [VERSION] and the next ## [ heading.
  # Includes the heading line itself.
  CHANGES="$(awk -v v="$VERSION" '
    BEGIN { capturing = 0; found = 0 }
    /^## \[/ {
      if (capturing) { capturing = 0; next }
      if ($0 ~ "\\[" v "\\]") { capturing = 1; found = 1; print; next }
    }
    capturing { print }
  ' CHANGELOG.md)"
  if [ -z "$CHANGES" ] || [ -z "$found" ]; then
    warn "no ## [$VERSION] section found in CHANGELOG.md — using template as-is"
    if $DRY_RUN; then
      dim "  would copy $RELEASE_NOTES_TEMPLATE → $RELEASE_NOTES_FILE"
    else
      cp "$RELEASE_NOTES_TEMPLATE" "$RELEASE_NOTES_FILE"
    fi
  else
    # Build the final release notes: changelog section + horizontal rule + template boilerplate.
  TEMPLATE_CONTENT="$(cat "$RELEASE_NOTES_TEMPLATE")"
    NOTES="${CHANGES}

---

${TEMPLATE_CONTENT}"
    write_file "$RELEASE_NOTES_FILE" "$NOTES"
  fi
  ok "$RELEASE_NOTES_FILE generated"
elif [ -f "$RELEASE_NOTES_FILE" ]; then
  say "Generating $RELEASE_NOTES_FILE"
  warn "$RELEASE_NOTES_TEMPLATE not found — leaving $RELEASE_NOTES_FILE unchanged"
fi

# ── Refresh Cargo.lock ──────────────────────────────────────────────────────
say "Refreshing Cargo.lock"
if $DRY_RUN; then
  dim "  would run: cargo update --workspace --offline (or fetch)"
else
  # `cargo check` is the cheapest way to record the new version in Cargo.lock.
  cargo check --quiet
fi
ok "Cargo.lock up to date"

# ── Tests, clippy, release build ────────────────────────────────────────────
if $SKIP_TESTS; then
  warn "skipping tests (--skip-tests)"
else
  say "Running cargo test"
  if ! $DRY_RUN; then
    cargo test --quiet
  fi
  ok "tests passed"
fi

say "Running cargo clippy"
if ! $DRY_RUN; then
  cargo clippy --quiet --all-targets -- -D warnings
fi
ok "clippy clean"

say "Building release"
if ! $DRY_RUN; then
  cargo build --quiet --release
fi
ok "release build OK"

# ── Commit ──────────────────────────────────────────────────────────────────
say "Committing"
if $DRY_RUN; then
  dim "  would: git add Cargo.toml Cargo.lock npm/package.json CHANGELOG.md install/statico.rb .github/release-notes.md"
  dim "  would: git commit -m 'chore: release $TAG'"
else
  git add Cargo.toml Cargo.lock 2>/dev/null || true
  [ -f npm/package.json ]      && git add npm/package.json
  [ -f CHANGELOG.md ]          && git add CHANGELOG.md
  [ -f install/statico.rb ]    && git add install/statico.rb
  [ -f .github/release-notes.md ] && git add .github/release-notes.md
  if git diff --cached --quiet; then
    die "nothing staged — version files were already up to date?"
  fi
  git commit -m "chore: release $TAG"
fi
ok "commit created"

# ── Tag ─────────────────────────────────────────────────────────────────────
say "Tagging"
if $DRY_RUN; then
  dim "  would: git tag -a $TAG -m 'Release $TAG'"
else
  git tag -a "$TAG" -m "Release $TAG"
fi
ok "annotated tag $TAG created"

# ── Push (optional) ─────────────────────────────────────────────────────────
if $PUSH; then
  say "Pushing"
  if $DRY_RUN; then
    dim "  would: git push origin $CURRENT_BRANCH"
    dim "  would: git push origin $TAG"
  else
    git push origin "$CURRENT_BRANCH"
    git push origin "$TAG"
  fi
  ok "pushed branch and tag"
  cat <<EOF

  ${C_GRN}✓ release $TAG pushed.${C_RST}
  • CI:      ${C_DIM}gh run watch${C_RST}
  • Release: ${C_DIM}gh release view $TAG${C_RST}
  • Once the release workflow finishes, the tarballs + SHASUMS256.txt are at:
      https://github.com/DonaldMurillo/statico/releases/tag/$TAG
EOF
else
  cat <<EOF

  ${C_GRN}✓ release $TAG prepared locally.${C_RST}
  Next:
    git push origin $CURRENT_BRANCH
    git push origin $TAG
  Or rerun with --push to do both for you.

  After the push, the release workflow will build tarballs and create the GitHub release.
  Watch with: gh run watch
EOF
fi
