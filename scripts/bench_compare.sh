#!/usr/bin/env bash
# bench_compare.sh — Run cargo bench, save results as JSON, optionally compare against a baseline.
#
# Usage:
#   ./scripts/bench_compare.sh                  # run benchmarks and save results
#   ./scripts/bench_compare.sh baseline.json    # run, save, and compare against baseline
#
# Regressions > 10% are flagged in the comparison output.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
RESULTS_DIR="$REPO_ROOT/benchmarks/results"
TIMESTAMP=$(date -u +"%Y%m%dT%H%M%SZ")
RESULT_FILE="$RESULTS_DIR/bench_${TIMESTAMP}.json"

mkdir -p "$RESULTS_DIR"

echo "==> Running cargo bench …"
# Capture criterion output (stdout) while still showing it live.
BENCH_OUTPUT=$(cargo bench --manifest-path "$REPO_ROOT/Cargo.toml" 2>&1 | tee /dev/stderr || true)

# ---- Parse criterion output into JSON ----
# Criterion prints lines like:
#   extract_imports           time:   [4.1234 µs 4.2345 µs 4.3456 µs]
# We extract benchmark name and the median (middle) estimate.

declare -a JSON_ENTRIES=()
while IFS= read -r line; do
    # Match lines with "time:   [" which is criterion's estimate output
    if echo "$line" | grep -qE 'time:\s+\['; then
        # Extract benchmark name (first field) and the three time estimates
        BENCH_NAME=$(echo "$line" | awk '{print $1}')
        # Extract the numbers between brackets: "lower median upper"
        TIMES=$(echo "$line" | sed -E 's/.*time:\s+\[([^]]+)\].*/\1/')
        LOWER=$(echo "$TIMES" | awk '{print $1}')
        MEDIAN=$(echo "$TIMES" | awk '{print $2}')
        UPPER=$(echo "$TIMES" | awk '{print $3}')
        # Extract the unit (µs, ms, ns, s) — look for it after the number
        UNIT=$(echo "$line" | sed -E 's/.*time:\s+\[([^]]+)\].*/\1/' | grep -oE '[a-zµ]+')
        # Use just the median for comparison; store unit separately
        JSON_ENTRIES+=("{\"name\":\"$BENCH_NAME\",\"median\":\"${MEDIAN}\",\"lower\":\"${LOWER}\",\"upper\":\"${UNIT}\",\"unit\":\"${UNIT}\"}")
    fi
done <<< "$BENCH_OUTPUT"

# Build JSON array
JSON="["
FIRST=true
for entry in "${JSON_ENTRIES[@]}"; do
    if [ "$FIRST" = true ]; then
        FIRST=false
    else
        JSON+=","
    fi
    JSON+="$entry"
done
JSON+="]"

# Write result file with metadata
cat > "$RESULT_FILE" <<HEREDOC
{
  "timestamp": "$TIMESTAMP",
  "date": "$(date -u +"%Y-%m-%d %H:%M:%S UTC")",
  "commit": "$(cd "$REPO_ROOT" && git rev-parse --short HEAD 2>/dev/null || echo unknown)",
  "branch": "$(cd "$REPO_ROOT" && git rev-parse --abbrev-ref HEAD 2>/dev/null || echo unknown)",
  "results": $JSON
}
HEREDOC

echo ""
echo "==> Results saved to $RESULT_FILE"

# ---- Compare against baseline if provided ----
BASELINE="${1:-}"
if [ -n "$BASELINE" ]; then
    if [ ! -f "$BASELINE" ]; then
        echo "ERROR: Baseline file '$BASELINE' not found." >&2
        exit 1
    fi

    echo ""
    echo "==> Comparing against baseline: $BASELINE"
    echo ""

    # Use the Rust helper if available, otherwise fall back to jq
    if command -v jq &>/dev/null; then
        REGRESSIONS=0
        # For each benchmark in the current results, find the same in baseline
        for entry in "${JSON_ENTRIES[@]}"; do
            BENCH_NAME=$(echo "$entry" | jq -r '.name')
            CURRENT_MEDIAN=$(echo "$entry" | jq -r '.median')
            UNIT=$(echo "$entry" | jq -r '.unit')

            BASELINE_MEDIAN=$(jq -r --arg name "$BENCH_NAME" \
                '.results[] | select(.name == $name) | .median' "$BASELINE" 2>/dev/null || echo "")

            if [ -n "$BASELINE_MEDIAN" ] && [ "$BASELINE_MEDIAN" != "null" ]; then
                # Convert both to a common numeric value (strip units handled by jq comparison)
                # Numbers may have decimal points
                CURRENT_NUM=$(echo "$CURRENT_MEDIAN" | sed 's/[a-zµ]//g')
                BASELINE_NUM=$(echo "$BASELINE_MEDIAN" | sed 's/[a-zµ]//g')

                if command -v bc &>/dev/null; then
                    RATIO=$(echo "scale=4; $CURRENT_NUM / $BASELINE_NUM" | bc 2>/dev/null || echo "0")
                    PCT_CHANGE=$(echo "scale=2; ($CURRENT_NUM - $BASELINE_NUM) / $BASELINE_NUM * 100" | bc 2>/dev/null || echo "0")
                else
                    # Rough awk fallback
                    RATIO=$(awk "BEGIN {printf \"%.4f\", $CURRENT_NUM / $BASELINE_NUM}")
                    PCT_CHANGE=$(awk "BEGIN {printf \"%.2f\", ($CURRENT_NUM - $BASELINE_NUM) / $BASELINE_NUM * 100}")
                fi

                # Flag if > 10% slower
                REGRESSION=""
                HAS_PERL=""
                if command -v perl &>/dev/null; then
                    HAS_PERL=1
                    IS_REGRESSION=$(perl -e "print 1 if abs($PCT_CHANGE) > 10")
                else
                    IS_REGRESSION=$(awk "BEGIN {print (abs($PCT_CHANGE) > 10) ? 1 : 0}")
                fi

                if [ "$IS_REGRESSION" = "1" ]; then
                    REGRESSION=" ⚠️  REGRESSION"
                    REGRESSIONS=$((REGRESSIONS + 1))
                fi

                printf "  %-40s  %10s → %10s  (%+.2f%%)%s\n" \
                    "$BENCH_NAME" "$BASELINE_MEDIAN $UNIT" "$CURRENT_MEDIAN $UNIT" "$PCT_CHANGE" "$REGRESSION"
            else
                printf "  %-40s  %10s  (new benchmark, no baseline)\n" "$BENCH_NAME" "$CURRENT_MEDIAN $UNIT"
            fi
        done

        echo ""
        if [ "$REGRESSIONS" -gt 0 ]; then
            echo "⚠️  $REGRESSIONS regression(s) detected (> 10% change)."
            exit 2
        else
            echo "✅ No regressions detected."
        fi
    else
        echo "NOTE: Install 'jq' for detailed comparison. Showing raw files instead."
        echo "  Current:  $RESULT_FILE"
        echo "  Baseline: $BASELINE"
    fi
fi

echo ""
echo "Done. Results file: $RESULT_FILE"
