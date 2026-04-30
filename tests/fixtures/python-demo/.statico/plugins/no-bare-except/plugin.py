#!/usr/bin/env python3
"""no-bare-except plugin for statico — raw JSON-RPC 2.0 over stdin/stdout.

No SDK needed. Any language that can read lines from stdin and write
lines to stdout can be a statico plugin.
"""

import json
import sys

PLUGIN_NAME = "no-bare-except"
PLUGIN_VERSION = "1.0.0"


def handle_init(msg_id):
    return {
        "jsonrpc": "2.0",
        "id": msg_id,
        "result": {
            "name": PLUGIN_NAME,
            "version": PLUGIN_VERSION,
            "hooks": {"analyze_file": "add"},
            "languages": ["python"],
            "rules": [
                {
                    "id": "no-bare-except",
                    "severity": "warning",
                    "description": "Detect bare 'except:' clauses — catch specific exceptions instead",
                }
            ],
        },
    }


def handle_analyze_file(msg_id, params):
    source = params.get("source", "")
    file_path = params.get("path", "")
    issues = []

    lines = source.split("\n")
    for i, line in enumerate(lines):
        stripped = line.strip()
        # Skip comments
        if stripped.startswith("#"):
            continue
        # Detect bare except: (no exception type specified)
        # Matches "except:" but not "except ValueError:" or "except (A, B):"
        # Strip inline comments for cleaner matching.
        if stripped.startswith("except"):
            # Get the part after 'except'
            after_except = stripped[len("except"):].strip()
            # after_except could be ":", ":  # comment", "ValueError:", etc.
            # A bare except has nothing between 'except' and ':'
            # Remove inline comment for matching
            code_part = after_except.split("#")[0].strip()
            if code_part == ":" or code_part == "":
                issues.append(
                    {
                        "ruleId": "no-bare-except",
                        "severity": "warning",
                        "message": "Bare 'except:' catches all exceptions including KeyboardInterrupt and SystemExit",
                        "file": file_path,
                        "line": i + 1,
                        "column": line.index("except") + 1,
                        "confidence": 0.98,
                        "suggestion": "Catch a specific exception, e.g. 'except ValueError:' or 'except Exception:'",
                    }
                )

    return {"jsonrpc": "2.0", "id": msg_id, "result": {"issues": issues}}


def handle_shutdown(msg_id):
    return {"jsonrpc": "2.0", "id": msg_id, "result": None}


def main():
    """Read JSON-RPC from stdin, dispatch, write to stdout."""
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue

        try:
            msg = json.loads(line)
        except json.JSONDecodeError:
            sys.stdout.write(
                json.dumps(
                    {"jsonrpc": "2.0", "id": 0, "error": {"code": -32700, "message": "Parse error"}}
                )
                + "\n"
            )
            sys.stdout.flush()
            continue

        msg_id = msg.get("id", 0)
        method = msg.get("method", "")
        params = msg.get("params", {})

        if method == "init":
            resp = handle_init(msg_id)
        elif method == "shutdown":
            resp = handle_shutdown(msg_id)
            sys.stdout.write(json.dumps(resp) + "\n")
            sys.stdout.flush()
            sys.exit(0)
        elif method == "analyze_file":
            resp = handle_analyze_file(msg_id, params)
        else:
            resp = {
                "jsonrpc": "2.0",
                "id": msg_id,
                "error": {"code": -32601, "message": f"Method not found: {method}"},
            }

        sys.stdout.write(json.dumps(resp) + "\n")
        sys.stdout.flush()


if __name__ == "__main__":
    main()
