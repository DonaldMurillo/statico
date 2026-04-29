#!/usr/bin/env python3
"""
Public codebase benchmark: statico vs fallow vs knip vs ts-prune.

Repos are cloned into benchmarks/repos/ (gitignored) and kept up-to-date
by fetching latest main. Each repo tests a specific statico feature.

Usage:
    python3 benchmark-public.py              # run all
    python3 benchmark-public.py --repo nextjs # run one
    python3 benchmark-public.py --update      # force git pull
    python3 benchmark-public.py --init        # clone missing repos only
"""
import argparse
import json
import os
import subprocess
import sys
import time

PROJECT = os.path.dirname(os.path.abspath(__file__))
REPOS_DIR = os.path.join(PROJECT, "benchmarks", "repos")

# feature_key: (owner/repo, display_name, subpath_to_analyze)
# subpath=None means analyze the whole repo root
REPOS = {
    # Real apps that CONSUME frameworks, not framework source repos.
    # Each repo tests a specific statico feature.
    # subpath=None means analyze the whole repo root.

    "calcom": {
        "remote": "calcom/cal.com",
        "name": "Cal.com",
        "feature": "dead-code, Next.js, monorepo (turborepo)",
        "subpath": None,
        "shallow": True,
        "note": "Large Next.js turborepo monorepo with 1100+ .tsx files",
    },
    "shadcn": {
        "remote": "shadcn-ui/ui",
        "name": "shadcn/ui",
        "feature": "unused-exports",
        "subpath": None,
        "shallow": True,
        "note": "Component library — many exported components, tests unused export detection",
    },
    "nestjs": {
        "remote": "nestjs/nest",
        "name": "NestJS",
        "feature": "gotchas, NestJS framework detection",
        "subpath": "sample/01-cats-app",
        "shallow": True,
        "note": "Real NestJS sample app with controllers/modules/services",
    },
    "angular": {
        "remote": "angular/angular",
        "name": "Angular",
        "feature": "Angular framework detection",
        "subpath": None,
        "shallow": True,
        "note": "Angular monorepo — tests large monorepo analysis",
    },
    "turborepo": {
        "remote": "vercel/turborepo",
        "name": "Turborepo",
        "feature": "monorepo detection (pnpm workspaces)",
        "subpath": None,
        "shallow": True,
        "note": "Real pnpm monorepo with packages/ and apps/",
    },
    "payload": {
        "remote": "payloadcms/payload",
        "name": "Payload CMS",
        "feature": "Payload framework detection, duplication",
        "subpath": None,
        "shallow": True,
        "note": "Payload-based project with collections, endpoints, config",
    },
    "vue-docs": {
        "remote": "vuejs/docs",
        "name": "Vue.js Docs",
        "feature": "Vue framework detection",
        "subpath": "src",
        "shallow": True,
        "note": "Vue 3 docs site — 60+ .vue files with composition API",
    },
    "sveltekit": {
        "remote": "huntabyte/shadcn-svelte",
        "name": "shadcn-svelte",
        "feature": "Svelte framework detection",
        "subpath": None,
        "shallow": True,
        "note": "Real SvelteKit app (component library) with +page.svelte, +layout.svelte",
    },
    "remix": {
        "remote": "remix-run/remix",
        "name": "Remix",
        "feature": "Remix framework detection",
        "subpath": None,
        "shallow": True,
        "note": "Remix monorepo — tests route pattern recognition",
    },
    "ai-sdk": {
        "remote": "vercel/ai",
        "name": "AI SDK",
        "feature": "stress test, large monorepo",
        "subpath": None,
        "shallow": True,
        "note": "Large monorepo — stress test for performance and correctness",
    },
}

# ---------------------------------------------------------------------------

def run(cmd, cwd=None, timeout=120):
    """Run command, return (stdout, elapsed_ms). Returns '' on failure."""
    start = time.time()
    try:
        r = subprocess.run(cmd, capture_output=True, text=True, cwd=cwd, timeout=timeout)
        elapsed = int((time.time() - start) * 1000)
        return r.stdout.strip(), elapsed
    except subprocess.TimeoutExpired:
        elapsed = int((time.time() - start) * 1000)
        print(f"    ⏱ TIMEOUT ({timeout}s) for {' '.join(cmd[:3])}...")
        return "", elapsed
    except Exception as e:
        return f"ERROR: {e}", 0


def ensure_repo(key, cfg, update=False, init_only=False):
    """Clone or update a repo. Returns the local path."""
    remote = cfg["remote"]
    local = os.path.join(REPOS_DIR, key)
    url = f"https://github.com/{remote}.git"

    if not os.path.isdir(local):
        print(f"  📦 Cloning {remote}...")
        os.makedirs(REPOS_DIR, exist_ok=True)
        if cfg.get("shallow", True):
            out, _ = run(["git", "clone", "--depth", "1", url, local])
        else:
            out, _ = run(["git", "clone", url, local])
        if not os.path.isdir(local):
            print(f"    ❌ Failed to clone {remote}")
            return None
    elif update and not init_only:
        print(f"  🔄 Updating {remote}...")
        run(["git", "fetch", "origin"], cwd=local)
        run(["git", "reset", "--hard", "origin/HEAD"], cwd=local)

    return local


def run_statico(path):
    """Run statico, return (dead_count, unused_count, gotcha_count, time_ms)."""
    bin_path = os.path.join(PROJECT, "target", "release", "statico")
    if not os.path.isfile(bin_path):
        bin_path = os.path.join(PROJECT, "target", "debug", "statico")
    out, ms = run([bin_path, "analyze", path, "--format", "json"], timeout=600)
    if not out:
        return None, None, None, ms
    try:
        # Find the JSON object start (skip any progress lines)
        start_idx = out.find('{')
        if start_idx == -1:
            return None, None, None, ms
        # For large outputs, extract just the issues counts using string search
        # instead of parsing the full JSON
        issues_start = out.find('"issues"', start_idx)
        if issues_start == -1:
            return None, None, None, ms
        d = json.loads(out[start_idx:])
        issues = d.get("issues", {})
        dead = len(issues.get("dead_code", []))
        unused = len(issues.get("unused_exports", []))
        gotchas = len(issues.get("gotchas", []))
        return dead, unused, gotchas, ms
    except (json.JSONDecodeError, KeyError) as e:
        print(f"    ⚠ JSON parse error: {e}")
        return None, None, None, ms


def run_fallow(path):
    """Run fallow, return (dead_count, unused_count, time_ms)."""
    out, ms = run(["fallow", "dead-code", "--format", "json", "-q", "--root", path], timeout=120)
    if not out:
        return None, None, ms
    try:
        # Strip ANSI codes and non-JSON lines
        start = out.find('{')
        if start == -1:
            return None, None, ms
        d = json.loads(out[start:])
        summary = d.get("summary", {})
        dead = summary.get("unused_files", 0)
        unused = summary.get("unused_exports", 0)
        return dead, unused, ms
    except (json.JSONDecodeError, KeyError):
        return None, None, ms


def run_knip(path):
    """Run knip, return (unused_count, time_ms). knip needs node_modules."""
    # Only run if package.json exists
    if not os.path.isfile(os.path.join(path, "package.json")):
        return "skip", 0
    out, ms = run(["npx", "--yes", "knip", "--no-exit-code", "--reporter", "json"],
                  cwd=path, timeout=120)
    if not out:
        return None, ms
    try:
        d = json.loads(out.split("\n")[-1] if "\n" in out else out)
        # knip returns files with unused exports
        unused = d.get("files", [])
        return len(unused), ms
    except (json.JSONDecodeError, KeyError):
        return None, ms


def run_ts_prune(path):
    """Run ts-prune, return (unused_count, time_ms). Needs tsconfig."""
    if not os.path.isfile(os.path.join(path, "tsconfig.json")):
        return "skip", 0
    out, ms = run(["npx", "--yes", "ts-prune", path], timeout=120)
    if not out:
        return None, ms
    # ts-prune outputs one line per unused export
    lines = [l for l in out.split("\n") if l.strip() and "unused" not in l.lower()]
    # Count lines that look like file:line — export
    count = len([l for l in lines if ":" in l])
    return count, ms


# ---------------------------------------------------------------------------

def benchmark_one(key, cfg, update=False):
    """Run all tools against one repo. Returns result dict."""
    local = ensure_repo(key, cfg, update=update)
    if not local:
        return None

    subpath = cfg.get("subpath")
    subpath = cfg.get("subpath")
    if subpath:
        analyze_path = os.path.join(local, subpath)
    else:
        analyze_path = local
    name = cfg["name"]
    feature = cfg["feature"]

    print(f"\n{'='*60}")
    print(f"  {name} ({cfg['remote']}) — testing: {feature}")
    print(f"{'='*60}")

    # statico
    print(f"  ⚡ statico...", end="", flush=True)
    s_dead, s_unused, s_gotchas, s_ms = run_statico(analyze_path)
    print(f" {s_ms}ms (dead={s_dead}, unused={s_unused}, gotchas={s_gotchas})")

    # fallow
    print(f"  🍂 fallow...", end="", flush=True)
    f_dead, f_unused, f_ms = run_fallow(analyze_path)
    print(f" {f_ms}ms (dead={f_dead}, unused={f_unused})")

    # knip
    print(f"  ✂️  knip...", end="", flush=True)
    k_unused, k_ms = run_knip(analyze_path)
    if k_unused == "skip":
        print(f" skipped (no package.json)")
    else:
        print(f" {k_ms}ms (unused={k_unused})")

    # ts-prune
    print(f"  🔍 ts-prune...", end="", flush=True)
    tp_unused, tp_ms = run_ts_prune(analyze_path)
    if tp_unused == "skip":
        print(f" skipped (no tsconfig.json)")
    else:
        print(f" {tp_ms}ms (unused={tp_unused})")

    return {
        "key": key,
        "name": name,
        "remote": cfg["remote"],
        "feature": feature,
        "statico": {"dead": s_dead, "unused": s_unused, "gotchas": s_gotchas, "ms": s_ms},
        "fallow": {"dead": f_dead, "unused": f_unused, "ms": f_ms},
        "knip": {"unused": k_unused, "ms": k_ms},
        "ts_prune": {"unused": tp_unused, "ms": tp_ms},
    }


def print_summary(results):
    """Print a comparison table."""
    print(f"\n{'='*80}")
    print(f"  BENCHMARK SUMMARY")
    print(f"{'='*80}\n")

    # Header
    print(f"{'Repo':<20} {'Feature':<25} {'statico':>22} {'fallow':>16} {'knip':>12} {'ts-prune':>12}")
    print(f"{'─'*20} {'─'*25} {'─'*22} {'─'*16} {'─'*12} {'─'*12}")

    for r in results:
        if not r:
            continue
        s = r["statico"]
        f = r["fallow"]
        k = r["knip"]
        tp = r["ts_prune"]

        s_dead = s['dead'] if s['dead'] is not None else '?'
        s_unused = s['unused'] if s['unused'] is not None else '?'
        s_gotchas = s['gotchas'] if s['gotchas'] is not None else '?'
        s_str = f"d={s_dead} u={s_unused} g={s_gotchas} ({s['ms']}ms)"
        f_dead = f['dead'] if f['dead'] is not None else '?'
        f_unused = f['unused'] if f.get('unused') is not None else '?'
        f_str = f"d={f_dead} u={f_unused} ({f['ms']}ms)"
        k_unused = k['unused'] if k['unused'] is not None else '?'
        k_str = f"u={k_unused} ({k['ms']}ms)" if k['unused'] != "skip" else "skip"
        tp_unused = tp['unused'] if tp['unused'] is not None else '?'
        tp_str = f"u={tp_unused} ({tp['ms']}ms)" if tp['unused'] != "skip" else "skip"

        print(f"{r['name']:<20} {r['feature']:<25} {s_str:>20} {f_str:>12} {k_str:>12} {tp_str:>12}")

    # Speed comparison
    print(f"\n{'─'*80}")
    print(f"  SPEED (statico vs fallow):")
    for r in results:
        if not r:
            continue
        s_ms = r["statico"]["ms"]
        f_ms = r["fallow"]["ms"]
        ratio = f"{s_ms/f_ms:.0f}x" if f_ms and f_ms > 0 else "N/A"
        print(f"    {r['name']:<20} statico {s_ms}ms vs fallow {f_ms}ms ({ratio} slower)")


def main():
    parser = argparse.ArgumentParser(description="Public codebase benchmark")
    parser.add_argument("--repo", help="Run only one repo (key name)")
    parser.add_argument("--update", action="store_true", help="Force git pull on repos")
    parser.add_argument("--init", action="store_true", help="Only clone missing repos")
    parser.add_argument("--json", action="store_true", help="Output results as JSON")
    args = parser.parse_args()

    os.makedirs(REPOS_DIR, exist_ok=True)

    keys = [args.repo] if args.repo else list(REPOS.keys())
    results = []

    for key in keys:
        cfg = REPOS.get(key)
        if not cfg:
            print(f"Unknown repo: {key}")
            print(f"  Available: {', '.join(REPOS.keys())}")
            sys.exit(1)
        if args.init:
            ensure_repo(key, cfg, update=False, init_only=True)
        else:
            r = benchmark_one(key, cfg, update=args.update)
            results.append(r)

    if args.init:
        print(f"\n✅ All repos initialized in {REPOS_DIR}")
        return

    if args.json:
        # Clean up non-serializable values
        clean = []
        for r in results:
            if not r:
                continue
            for tool in ["knip", "ts_prune"]:
                if r[tool]["unused"] == "skip":
                    r[tool]["unused"] = None
            clean.append(r)
        print(json.dumps(clean, indent=2))
        return

    print_summary(results)

    # Save results
    results_file = os.path.join(PROJECT, "benchmarks", "results.json")
    os.makedirs(os.path.dirname(results_file), exist_ok=True)
    with open(results_file, "w") as f:
        json.dump(results, f, indent=2, default=str)
    print(f"\n💾 Results saved to {results_file}")


if __name__ == "__main__":
    main()
