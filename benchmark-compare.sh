#!/usr/bin/env bash
# Benchmark: statico vs fallow vs knip on all fixtures
set -euo pipefail

cd "$(dirname "$0")"

fixtures=(
  dead-code-project
  duplicate-exports-project
  nextjs-project
  payload-project
  angular-project
  nestjs-project
  pnpm-monorepo
  npm-monorepo
  nx-monorepo
  turborepo-monorepo
)

echo "# Static Analysis Tool Comparison"
echo "# Fixtures: ${fixtures[*]}"
echo ""

for fixture in "${fixtures[@]}"; do
  dir="fixtures/$fixture"
  if [ ! -d "$dir" ]; then
    echo "SKIP $fixture (no fixture dir)"
    continue
  fi

  echo "## $fixture"
  echo ""

  # --- statico ---
  echo "### statico"
  t_start=$(python3 -c "import time; print(time.time())")
  statico_out=$(cargo run -q -- analyze "$dir" --format json 2>/dev/null || echo '{"error": "failed"}')
  t_end=$(python3 -c "import time; print(time.time())")
  statico_ms=$(python3 -c "print(int(($t_end - $t_start) * 1000))")
  
  statico_files=$(echo "$statico_out" | python3 -c "import sys,json; d=json.load(sys.stdin); print(len(d.get('structure',{}).get('source_files',[])))" 2>/dev/null || echo "?")
  statico_dead=$(echo "$statico_out" | python3 -c "import sys,json; d=json.load(sys.stdin); print(len(d.get('issues',{}).get('dead_code',[])))" 2>/dev/null || echo "?")
  statico_unused=$(echo "$statico_out" | python3 -c "import sys,json; d=json.load(sys.stdin); print(len(d.get('issues',{}).get('unused_exports',[])))" 2>/dev/null || echo "?")
  statico_dup=$(echo "$statico_out" | python3 -c "import sys,json; d=json.load(sys.stdin); print(d.get('duplication',{}).get('duplication_percentage','?'))" 2>/dev/null || echo "?")
  statico_gotchas=$(echo "$statico_out" | python3 -c "import sys,json; d=json.load(sys.stdin); print(len(d.get('issues',{}).get('gotchas',[])))" 2>/dev/null || echo "?")
  statico_frameworks=$(echo "$statico_out" | python3 -c "import sys,json; d=json.load(sys.stdin); print(','.join(d.get('detected_frameworks',[])))" 2>/dev/null || echo "?")
  statico_monorepo=$(echo "$statico_out" | python3 -c "import sys,json; d=json.load(sys.stdin); m=d.get('monorepo'); print(m.get('kind','none') if m else 'none')" 2>/dev/null || echo "?")
  
  echo "  Time: ${statico_ms}ms | Files: $statico_files | Dead: $statico_dead | Unused exports: $statico_unused | Gotchas: $statico_gotchas | Dup%: $statico_dup | Frameworks: $statico_frameworks | Monorepo: $statico_monorepo"
  echo ""

  # --- fallow ---
  echo "### fallow"
  t_start=$(python3 -c "import time; print(time.time())")
  fallow_out=$(fallow dead-code --root "$dir" --format json 2>/dev/null || echo '{"error": "failed"}')
  t_end=$(python3 -c "import time; print(time.time())")
  fallow_ms=$(python3 -c "print(int(($t_end - $t_start) * 1000))")

  fallow_dead_files=$(echo "$fallow_out" | python3 -c "import sys,json; d=json.load(sys.stdin); unused=d.get('unusedFiles',[]); print(len(unused))" 2>/dev/null || echo "?")
  fallow_unused=$(echo "$fallow_out" | python3 -c "import sys,json; d=json.load(sys.stdin); print(len(d.get('unusedExports',[])))" 2>/dev/null || echo "?")
  fallow_cycles=$(echo "$fallow_out" | python3 -c "import sys,json; d=json.load(sys.stdin); print(len(d.get('circularDependencies',[])))" 2>/dev/null || echo "?")
  fallow_types=$(echo "$fallow_out" | python3 -c "import sys,json; d=json.load(sys.stdin); print(len(d.get('unusedTypes',[])))" 2>/dev/null || echo "?")
  
  echo "  Time: ${fallow_ms}ms | Dead files: $fallow_dead_files | Unused exports: $fallow_unused | Unused types: $fallow_types | Cycles: $fallow_cycles"
  echo ""

  # --- knip ---
  echo "### knip"
  t_start=$(python3 -c "import time; print(time.time())")
  knip_out=$(cd "$dir" && knip --no-progress --format json 2>/dev/null || echo '{"error": "failed"}')
  t_end=$(python3 -c "import time; print(time.time())")
  knip_ms=$(python3 -c "print(int(($t_end - $t_start) * 1000))")

  knip_unused=$(echo "$knip_out" | python3 -c "import sys,json; d=json.load(sys.stdin); print(len(d.get('files',[])))" 2>/dev/null || echo "?")
  knip_exports=$(echo "$knip_out" | python3 -c "import sys,json; d=json.load(sys.stdin); print(len(d.get('exports',[])))" 2>/dev/null || echo "?")
  knip_types=$(echo "$knip_out" | python3 -c "import sys,json; d=json.load(sys.stdin); print(len(d.get('types',[])))" 2>/dev/null || echo "?")
  
  echo "  Time: ${knip_ms}ms | Unused files: $knip_unused | Unused exports: $knip_exports | Unused types: $knip_types"
  echo ""
  echo "---"
  echo ""
done
