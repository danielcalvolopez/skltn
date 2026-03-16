import functools
import time


def timer(func):
    """Decorator that times function execution."""
    @functools.wraps(func)
    def wrapper(*args, **kwargs):
        start = time.perf_counter()
        result = func(*args, **kwargs)
        elapsed = time.perf_counter() - start
        print(f"{func.__name__} took {elapsed:.4f}s")
        return result
    return wrapper


def retry(max_attempts: int = 3):
    """Decorator factory for retry logic."""
    def decorator(func):
        @functools.wraps(func)
        def wrapper(*args, **kwargs):
            for attempt in range(max_attempts):
                try:
                    return func(*args, **kwargs)
                except Exception as e:
                    if attempt == max_attempts - 1:
                        raise
                    time.sleep(2 ** attempt)
        return wrapper
    return decorator


@timer
def slow_operation(data: list) -> list:
    """A slow operation that benefits from timing."""
    result = []
    for item in data:
        processed = item.strip().lower()
        result.append(processed)
        time.sleep(0.01)
    return result


@retry(max_attempts=5)
def fetch_with_retry(url: str) -> dict:
    """Fetch data from URL with automatic retries."""
    import urllib.request
    response = urllib.request.urlopen(url)
    return json.loads(response.read())
