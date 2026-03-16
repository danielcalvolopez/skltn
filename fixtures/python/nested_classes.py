class Outer:
    """Outer class with nested classes."""

    class Inner:
        """Inner configuration class."""
        value: int = 42

        def get_value(self) -> int:
            """Return the configured value."""
            return self.value * 2

    class AnotherInner:
        """Another nested class."""

        def compute(self, x: int) -> int:
            """Perform computation."""
            result = x ** 2
            if result > 1000:
                result = 1000
            return result

    def use_inner(self) -> int:
        """Use the inner class."""
        inner = self.Inner()
        return inner.get_value()
