class OuterService:
    """Service with deeply nested structures."""

    class Config:
        """Nested configuration."""
        timeout: int = 30
        retries: int = 3

    def process(self, data: dict) -> dict:
        """Process data with error handling."""
        try:
            if data.get("type") == "complex":
                for item in data["items"]:
                    if item.get("nested"):
                        for sub in item["nested"]:
                            result = self._transform(sub)
                            if not result:
                                raise ValueError(f"Transform failed for {sub}")
                            self._store(result)
            else:
                return self._simple_process(data)
        except KeyError as e:
            logger.error(f"Missing key: {e}")
            raise
        except ValueError:
            return {"status": "error", "data": data}
        return {"status": "ok"}

    def _transform(self, item: dict) -> Optional[dict]:
        mapped = {}
        for key, value in item.items():
            if isinstance(value, str):
                mapped[key] = value.strip().lower()
            elif isinstance(value, (int, float)):
                mapped[key] = value * 1.1
            else:
                mapped[key] = str(value)
        return mapped if mapped else None
