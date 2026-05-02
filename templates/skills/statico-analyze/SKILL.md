---
name: statico-analyze
description: Run statico code analysis on the current project. Use when asked to check code health, find dead code, review code quality, or analyze dependencies.
---

# statico-analyze

## When to Use

- User asks to check code health or code quality
- User wants to find dead code, unused exports, or circular dependencies
- User wants to understand the dependency graph
- Before/after refactoring to measure impact
- CI pipeline code quality gates

## Instructions

1. Run the analysis:
   ```bash
   statico analyze . --format ai
   ```
   The `--format ai` output is compressed for LLM context windows.

2. For detailed issue locations, use:
   ```bash
   statico analyze . --format context
   ```

3. For dependency visualization:
   ```bash
   statico analyze . --format mermaid
   ```

4. Interpret the results:
   - **Health score** (0–100): Overall code health. 80+ is good, 60–80 needs attention, <60 is critical.
   - **Dead code**: Files/exports that nothing references. Safe to remove.
   - **Unused exports**: Exports not imported anywhere. Consider making internal.
   - **Circular dependencies**: Files that import each other. Break with dependency injection or events.
   - **Duplication**: Code blocks duplicated across files. Extract shared utilities.
   - **Confidence** (0.0–1.0): How certain the detector is. Filter with `--min-confidence 0.7`.

5. To compare before/after changes:
   ```bash
   statico analyze . --format json > before.json
   # ... make changes ...
   statico analyze . --format json > after.json
   statico diff before.json after.json
   ```
