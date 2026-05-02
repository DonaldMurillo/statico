# Templates

Source-of-truth markdown content that `statico setup` writes into user projects.

## Layout

```
templates/
├── CLAUDE.md                              → .claude/CLAUDE.md  in user repo
├── skills/
│   ├── statico-analyze/SKILL.md           → .claude/ + .pi/ skills
│   ├── statico-fix/SKILL.md               → .claude/ + .pi/ skills
│   └── statico-plugin/SKILL.md            → .claude/ + .pi/ skills
└── cursor/
    └── statico.mdc                        → .cursor/rules/statico.mdc
```

## How it works

`src/commands/init.rs` reads each file at compile time via `include_str!` and
embeds it in the binary. `statico setup [--target claude|pi|cursor|all]` then
writes the embedded content to the matching paths in the user's project.

## Editing

Edit the markdown directly. The next `cargo build` picks up the change — no
intermediate code generation step.

## Why a separate directory

Keeping these as real markdown files (rather than inline `r#"..."#` literals
in Rust) means you can:

- See diffs as actual markdown when reviewing
- Lint with markdown tools, spellcheck, etc.
- Preview rendered output before shipping
- Let editors and AI assistants apply normal markdown features
