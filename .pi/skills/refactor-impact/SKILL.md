---
name: refactor-impact
description: Measure the impact of refactoring using statico before/after analysis. Snapshots code health before changes and compares after. Use when planning or completing a refactoring, measuring code quality improvements, or verifying cleanup effectiveness.
---

# /refactor-impact — Refactoring Impact Analysis

## Setup
Build statico if not already built:
```bash
cd /Users/dom/programming/statico && cargo build --release --features deep-resolution
```

## Workflow

### Before refactoring
```bash
/Users/dom/programming/statico/target/release/statico analyze . --format json --quiet > .statico/before.json
```
Parse the JSON and note:
- Health score
- Dead code count
- Unused exports count
- Circular deps count
- Duplication %

### After refactoring
```bash
/Users/dom/programming/statico/target/release/statico analyze . --format json --quiet > .statico/after.json
```

### Compare
```bash
/Users/dom/programming/statico/target/release/statico diff .statico/before.json .statico/after.json
```

Present the diff results:
- Health score change (+/-)
- Issues fixed / issues introduced
- LOC removed / added
- Duplication change
- Summary: "Refactoring improved health by {N} points, removed {M} dead files"

## Example Output
```
📊 Refactoring Impact Report
Health: 53.9 → 68.2 (+14.3)
Dead code: 12 → 3 (-9 files)
Unused exports: 847 → 612 (-235)
Duplication: 18.2% → 14.1% (-4.1%)
```

## One-liner for quick check
```bash
/Users/dom/programming/statico/target/release/statico analyze . --format context --quiet
```
