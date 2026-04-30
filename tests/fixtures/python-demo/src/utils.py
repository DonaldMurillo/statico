"""Clean Python file — no bare excepts."""

import json


def parse_json(text: str) -> dict:
    try:
        return json.loads(text)
    except json.JSONDecodeError:
        return {}


def merge_dicts(a: dict, b: dict) -> dict:
    return {**a, **b}
