#!/usr/bin/env python3
"""Compare statico vs fallow vs knip on all fixtures."""
import json
import os
import subprocess
import time

PROJECT = os.path.dirname(os.path.abspath(__file__))
FIXTURES = [
    "dead-code-project",
    "duplicate-exports-project", 
    "nextjs-project",
    "payload-project",
    "angular-project",
    "nestjs-project",
    "pnpm-monorepo",
    "npm-monorepo",
    "nx-monorepo",
    "turborepo-monorepo",
    "vue-project",
    "svelte-project",
    "remix-project",
    "astro-project",
    "barrel-chain-project",
    "dynamic-imports-project",
    "circular-deps-project",
    "type-only-project",
    "realworld-app",
]

def run(cmd, cwd=None):
    """Run command, return (stdout, elapsed_ms)."""
    start = time.time()
    try:
        r = subprocess.run(cmd, capture_output=True, text=True, cwd=cwd, timeout=30)
        elapsed = int((time.time() - start) * 1000)
        return r.stdout, elapsed
    except Exception as e:
        return str(e), 0

def parse_statico(output):
    try:
        # Find JSON start (skip any warnings on stdout)
        idx = output.find('{')
        if idx < 0:
            return {}
        d = json.loads(output[idx:])
        return d
    except:
        return {}

def parse_fallow(output):
    try:
        idx = output.find('{')
        if idx < 0:
            return {}
        d = json.loads(output[idx:])
        return d
    except:
        return {}

def parse_knip(output):
    try:
        idx = output.find('{')
        if idx < 0:
            return {}
        d = json.loads(output[idx:])
        return d
    except:
        return {}

print(f"# Static Analysis Comparison: statico vs fallow vs knip")
print(f"# {len(FIXTURES)} fixtures\n")
print(f"| Fixture | Tool | Time | Dead files | Unused exports | Gotchas/Types | Dup% | Framework | Monorepo |")
print(f"|---------|------|------|------------|----------------|---------------|------|-----------|----------|")

for fixture in FIXTURES:
    fdir = os.path.join(PROJECT, "fixtures", fixture)
    if not os.path.isdir(fdir):
        continue
    
    # --- statico ---
    out, ms = run(["cargo", "run", "-q", "--", "analyze", fdir, "--format", "json"], cwd=PROJECT)
    d = parse_statico(out)
    s_files = len(d.get("structure", {}).get("source_files", []))
    s_dead = len(d.get("issues", {}).get("dead_code", []))
    s_unused = len(d.get("issues", {}).get("unused_exports", []))
    s_gotchas = len(d.get("issues", {}).get("gotchas", []))
    s_dup = d.get("duplication", {}).get("duplication_percentage", "?")
    s_fw = ",".join(d.get("detected_frameworks", []) or [])
    s_mono = (d.get("monorepo") or {}).get("kind", "none")
    
    # Detail: list dead code files
    s_dead_list = [x["path"] for x in d.get("issues", {}).get("dead_code", [])]
    
    print(f"| {fixture} | **statico** | {ms}ms | {s_dead} | {s_unused} | {s_gotchas} gotchas | {s_dup}% | {s_fw} | {s_mono} |")
    
    # --- fallow ---
    out2, ms2 = run(["fallow", "dead-code", "--root", fdir, "--format", "json"], cwd=PROJECT)
    f = parse_fallow(out2)
    f_dead_files = len(f.get("unused_files", []))
    f_unused = len(f.get("unused_exports", []))
    f_types = len(f.get("unused_types", []))
    f_cycles = len(f.get("circular_dependencies", []))
    f_total = f.get("summary", {}).get("total_issues", "?")
    f_dead_list = [x["path"] for x in f.get("unused_files", [])]
    
    print(f"| | **fallow** | {ms2}ms | {f_dead_files} | {f_unused} | {f_types} types | - | auto | auto |")
    
    # --- knip ---
    out3, ms3 = run(["knip", "--directory", fdir, "--reporter", "json", "--no-progress"], cwd=PROJECT)
    k = parse_knip(out3)
    k_issues = k.get("issues", [])
    k_unused_files = sum(1 for i in k_issues if i.get("files"))
    k_exports = sum(len(i.get("exports", [])) for i in k_issues)
    k_types = sum(len(i.get("types", [])) for i in k_issues)
    k_dead_list = [i["file"] for i in k_issues if i.get("files")]
    
    print(f"| | **knip** | {ms3}ms | {k_unused_files} | {k_exports} | {k_types} types | - | auto | auto |")
    
    # Agreement analysis
    s_set = set(s_dead_list)
    f_set = set(f_dead_list)
    k_set = set(k_dead_list)
    
    if s_set or f_set or k_set:
        all_found = s_set | f_set | k_set
        all_agree = s_set & f_set & k_set
        only_s = s_set - f_set - k_set
        only_f = f_set - s_set - k_set
        only_k = k_set - s_set - f_set
        print(f"| | *agreement* | | *all 3 agree: {len(all_agree)}* | *only statico: {len(only_s)}* | *only fallow: {len(only_f)}* | *only knip: {len(only_k)}* | | |")
    
    print()

# Summary of false positives
print("\n## Detailed Dead Code Findings\n")
for fixture in FIXTURES:
    fdir = os.path.join(PROJECT, "fixtures", fixture)
    if not os.path.isdir(fdir):
        continue
    
    print(f"### {fixture}")
    
    out, _ = run(["cargo", "run", "-q", "--", "analyze", fdir, "--format", "json"], cwd=PROJECT)
    d = parse_statico(out)
    for item in d.get("issues", {}).get("dead_code", []):
        print(f"  - statico: {item['path']} (conf: {item['confidence']})")
    
    out2, _ = run(["fallow", "dead-code", "--root", fdir, "--format", "json"], cwd=PROJECT)
    f = parse_fallow(out2)
    for item in f.get("unused_files", []):
        print(f"  - fallow:  {item['path']}")
    
    out3, _ = run(["knip", "--directory", fdir, "--reporter", "json", "--no-progress"], cwd=PROJECT)
    k = parse_knip(out3)
    for issue in k.get("issues", []):
        if issue.get("files"):
            print(f"  - knip:    {issue['file']}")
    
    print()
