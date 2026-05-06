---
name: cleanup
description: Reclaim disk space by cleaning build artifacts, dependency caches, and benchmark repos. Auto-invokes before merges, releases, and after big branch switches — or whenever the user asks to clean up, free space, or trim the repo.
---

# cleanup

Reclaim disk space eaten by Rust build artifacts, node_modules, benchmark repos, and other transient files.

## When to use

**Auto-invoke proactively** when:

- About to merge a PR or switch to a long-lived branch (`main`, `release/*`)
- About to run `scripts/release.sh` (pairs with the `release` skill)
- After finishing a feature branch and before checking out a new one
- The user hasn't cleaned up in a while and disk space matters
- The user says "clean up", "free space", "trim", "slim down", "cargo clean", or "this repo is huge"

**Do NOT auto-invoke** when:

- In the middle of an active compile/test cycle (you'd delete artifacts being used)
- On a CI runner (their cleanup is different)

## What you'll clean

### Tier 1 — Always safe (rebuild in seconds)

| Target | Typical size | Command |
|---|---|---|
| Debug build artifacts | ~16 GB | `cargo clean` |
| Release build artifacts | ~1.2 GB | (included in `cargo clean`) |

```bash
cargo clean
```

> **Note:** This wipes **all** build artifacts (debug + release). The next `cargo build` or `cargo test` will recompile from scratch (~2-5 min). Only run this when the user isn't mid-compile.

### Tier 2 — Safe if you're not benchmarking

| Target | Typical size | Command |
|---|---|---|
| Benchmark repos | ~1.1 GB | `rm -rf benchmarks/repos/` |
| Benchmark results | ~2.4 MB | `rm -rf benchmarks/rust-results/` |

```bash
rm -rf benchmarks/repos/ benchmarks/rust-results/
```

> These are cloned/generated on demand by the benchmark runner. Safe to delete anytime.

### Tier 3 — Only if you're not working on the website

| Target | Typical size | Command |
|---|---|---|
| `website/node_modules/` | ~272 MB | `rm -rf website/node_modules/` |
| `website/dist/` | ~6 MB | `rm -rf website/dist/` |
| `website/.next/` | varies | `rm -rf website/.next/` |

```bash
rm -rf website/node_modules/ website/dist/ website/.next/
```

> Reinstall with `cd website && npm install`.

### Tier 4 — SDK build artifacts

| Target | Typical size | Command |
|---|---|---|
| `sdks/rust/target/` | ~94 MB | `cd sdks/rust && cargo clean` |
| `sdks/typescript/node_modules/` | ~32 MB | `rm -rf sdks/typescript/node_modules/` |

### Tier 5 — Git metadata (rarely needed)

| Target | Typical size | Command |
|---|---|---|
| `.git/` repack | varies | `git gc --aggressive --prune=now` |

> Only if the user explicitly asks. Git history is precious.

## Workflow

1. **Show the current footprint.** Run:

   ```bash
   du -sh . target/ benchmarks/repos/ website/node_modules/ sdks/
   ```

   Give the user a quick summary: "Repo is X GB, Y GB is reclaimable."

2. **Pick the right tier.** Ask what they want cleaned, or suggest based on context:

   - **Before merge/release** → Tier 1 + Tier 2 ("full clean, you'll rebuild after switch")
   - **Just freeing space** → Tier 1 ("cargo clean gets you 17 GB back")
   - **Not benchmarking or working on website** → Tiers 1-3 ("kitchen sink")
   - **Maximum nuclear** → All tiers

3. **Confirm before deleting.** Show exactly what will be removed and how much space it'll free. Get a yes/no.

4. **Run the cleanup.** Execute the appropriate commands.

5. **Show the after state.** Run `du -sh .` again and report the savings.

## Typical savings

| Scenario | Before | After | Saved |
|---|---|---|---|
| `cargo clean` only | 19 GB | ~2 GB | ~17 GB |
| Full clean (all tiers) | 19 GB | ~35 MB | ~19 GB |
| Just benchmarks + website | 19 GB | ~17.6 GB | ~1.4 GB |

## Safety rules

- **Never delete `.git/`** or any git refs
- **Never delete tracked source files** — only build output and generated/cloned content
- **Never run `cargo clean` during an active `cargo build/test`** — check with the user first
- **Always confirm** before deleting, even for "obviously safe" targets
- If the user is mid-work on a benchmark or website feature, skip those tiers
