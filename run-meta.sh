#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"
./target/debug/statico analyze /Users/dom/programming/products/metacollector > /Users/dom/programming/statico/metacollector-report.json 2>&1
echo "done"
