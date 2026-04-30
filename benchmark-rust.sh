#!/bin/bash
# Benchmark statico on Rust repos
set -e

STATICO="./target/release/statico"
REPOS_DIR="benchmarks/rust-repos"
RESULTS_DIR="benchmarks/rust-results"
mkdir -p "$RESULTS_DIR"

for repo in "$REPOS_DIR"/*/; do
    name=$(basename "$repo")
    echo "=== $name ==="
    
    # Count .rs files and LOC
    rs_count=$(find "$repo" -name "*.rs" -not -path "*/.git/*" | wc -l | tr -d ' ')
    loc=$(find "$repo" -name "*.rs" -not -path "*/.git/*" -exec cat {} + 2>&1 | wc -l | tr -d ' ')
    echo "  Files: $rs_count, LOC: $loc"
    
    # Create per-repo exclude config
    cat > "$repo/.statico.toml" << 'CFG'
exclude = [".git/**", "target/**"]
CFG
    
    # Run statico with timing
    start=$(date +%s%N)
    output=$($STATICO analyze "$repo" --format json --quiet 2>&1)
    end=$(date +%s%N)
    elapsed=$(( (end - start) / 1000000 ))
    
    # Parse results
    echo "$output" > "$RESULTS_DIR/${name}.json"
    
    echo "$output" | python3 -c "
import sys, json
text = sys.stdin.read()
try:
    d = json.loads(text[text.find('{'):])
    total = d['summary']['total_files']
    print(f'  Analyzed files: {total}')
    print(f'  Time: ${elapsed}ms')
    for k, v in d['issues'].items():
        if v:
            print(f'  {k}: {len(v)}')
except Exception as e:
    print(f'  ERROR: {e}')
" 2>&1
    
    echo ""
done
