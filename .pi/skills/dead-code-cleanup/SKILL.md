---
name: dead-code-cleanup
description: Interactive dead code cleanup using statico analysis. Identifies dead files, presents them with context, and helps safely remove them one by one. Use when asked to clean up dead code, remove unused files, or reduce codebase size.
---

# /dead-code-cleanup — Interactive Dead Code Removal

## Setup
Build statico if not already built:
```bash
cd /Users/dom/programming/statico && cargo build --release --features deep-resolution
```

## Workflow

### Step 1: Analyze
```bash
/Users/dom/programming/statico/target/release/statico analyze . --format ai --quiet
```

### Step 2: Present findings
For each dead file:
- Show file path, LOC, confidence, reason
- Show what imports this file (from the dependency graph)
- Assess safety: confidence >= 0.95 = safe to delete, < 0.95 = needs review
- Show the file content (first 20 lines) for context

### Step 3: Interactive cleanup
For each dead file, ask the user:
- "Delete {file}? ({LOC} LOC, confidence: {conf})"
- If yes: `git rm {file}`
- If no: skip

### Step 4: Verify
Re-run analysis to confirm cleanup was successful:
```bash
/Users/dom/programming/statico/target/release/statico analyze . --format context --quiet
```

### Step 5: Commit
```bash
git add -A && git commit -m "chore: remove {N} dead files ({LOC} LOC) identified by statico"
```

## Safety Rules
- NEVER delete files with confidence < 0.9 without explicit user approval
- NEVER delete test files, config files, or build files
- ALWAYS show the file content before proposing deletion
- ALWAYS verify with a re-run after cleanup
