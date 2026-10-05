import json

DEFAULTS = {"currency": "USD", "page_size": 20}


def load_config(path):
    with open(path, encoding="utf-8") as handle:
        return {**DEFAULTS, **json.load(handle)}
