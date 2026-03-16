from typing import Callable

# Lambdas should be emitted verbatim
double = lambda x: x * 2
greet = lambda name: f"Hello, {name}!"

TRANSFORMS: dict[str, Callable] = {
    "upper": lambda s: s.upper(),
    "lower": lambda s: s.lower(),
    "strip": lambda s: s.strip(),
}


def apply_transforms(data: list[str], transform_name: str) -> list[str]:
    """Apply a named transform to all items."""
    transform = TRANSFORMS.get(transform_name)
    if transform is None:
        raise ValueError(f"Unknown transform: {transform_name}")
    return [transform(item) for item in data]


def sort_by_length(items: list[str]) -> list[str]:
    """Sort items by string length."""
    return sorted(items, key=lambda s: len(s))
