---
description: Fix code quality issues found by statico. Use after running statico analyze to automatically address dead code, unused exports, and other detected issues.
---

# statico-fix

## When to Use

- After running statico analyze and getting issues
- User asks to fix, clean up, or resolve detected code quality problems
- User wants to remove dead code or unused exports

## Instructions

1. First, get the list of issues:
   ```bash
   statico analyze . --format fix
   ```
   This outputs machine-readable fix suggestions.

2. For each issue type:

   ### Dead Code (unreachable files)
   - Verify the file is truly unused (check dynamic imports, config references)
   - Delete the file
   - Remove any related test files

   ### Unused Exports
   - If the export is only used internally, remove the `export` keyword
   - If nothing uses it, remove the entire function/class/constant
   - For TypeScript, also check if the type is used in `.d.ts` files

   ### Circular Dependencies
   - Identify the cycle from the mermaid graph: `statico analyze . --format mermaid`
   - Break the cycle by extracting shared logic to a third file
   - Or use dependency injection / event patterns

   ### Code Duplication
   - Extract duplicated code into a shared utility
   - If the duplication is in tests, consider test helpers

3. After fixing, re-run to verify:
   ```bash
   statico analyze . --format ai
   ```
   Health score should improve.
