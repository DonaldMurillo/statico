# Test the resolver by checking what aliases are loaded for Cal.com
import subprocess, json

result = subprocess.run(
    ["./target/release/statico", "analyze", "benchmarks/repos/calcom", "--format", "json"],
    capture_output=True, text=True, timeout=120
)

# Count how many imports are resolved by checking the imported_names
# Look at unused exports in packages/ui
d = json.loads(result.stdout)
unused = d['issues']['unused_exports']
pkg_ui = [u for u in unused if 'packages/ui/' in u['path']]
print(f'Unused in packages/ui: {len(pkg_ui)}')
# Show top files
from collections import Counter
files = Counter(u['path'] for u in pkg_ui)
for path, count in files.most_common(5):
    print(f'  {path}: {count} unused')
