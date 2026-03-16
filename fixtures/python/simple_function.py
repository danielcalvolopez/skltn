from pathlib import Path
import json


def read_config(path: str) -> dict:
    """Read and parse a JSON configuration file."""
    with open(path) as f:
        data = json.load(f)
    validated = validate_config(data)
    return validated


def validate_config(data: dict) -> dict:
    required_keys = ["name", "version", "entries"]
    for key in required_keys:
        if key not in data:
            raise ValueError(f"Missing required key: {key}")
    if not isinstance(data["entries"], list):
        raise TypeError("entries must be a list")
    return data
