"""Sample Python project to test the no-bare-except plugin."""


def divide(a: int, b: int) -> float:
    try:
        return a / b
    except:  # should be flagged — bare except
        return 0.0


def safe_get(data: dict, key: str, default=None):
    try:
        return data[key]
    except KeyError:  # clean — specific exception
        return default


def risky_parse(text: str):
    try:
        return int(text)
    except:  # should be flagged — bare except
        print("parse failed")
        return None


def clean_function(x: int) -> int:
    """No try/except at all — perfectly clean."""
    return x * 2
