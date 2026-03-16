def valid_function(x: int) -> int:
    """A valid function."""
    return x + 1


def another_valid(s: str) -> str:
    return s.upper()


# This has a syntax error
def broken(x: int ->:
    return x


def after_error(y: int) -> int:
    return y * 2
