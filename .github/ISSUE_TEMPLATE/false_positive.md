---
name: False positive / false negative
about: statico flagged something it shouldn't, or missed something it should have caught
labels: false-positive
---

**Issue category**

- [ ] dead code
- [ ] unused export
- [ ] unused type
- [ ] unused dependency
- [ ] circular dependency
- [ ] duplicate code
- [ ] gotcha / framework rule
- [ ] other:

**What statico reported (or didn't)**

<!-- Paste the relevant section from `--format markdown` output. -->

**Why it's wrong**

<!-- e.g. the export *is* used here: <link to file:line>. Or: this file is
     reachable via <framework feature> but statico didn't pick it up. -->

**Minimal repro**

<!-- A small project + the exact `statico analyze ...` command. -->
