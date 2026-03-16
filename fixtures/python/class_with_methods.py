from typing import Optional
import logging

logger = logging.getLogger(__name__)

TIMEOUT = 30
MAX_RETRIES = 3


class UserService:
    """Manages user operations and authentication."""

    def __init__(self, db_url: str, timeout: int = TIMEOUT):
        """Initialize the service with a database connection."""
        self.db_url = db_url
        self.timeout = timeout
        self._connection = None
        self._cache: dict = {}
        logger.info(f"UserService initialized with {db_url}")

    def authenticate(self, token: str) -> bool:
        """Validate a user token against the database."""
        decoded = self._decode_token(token)
        if not decoded:
            return False
        user = self._fetch_user(decoded["sub"])
        if user and user.is_active:
            self._cache[token] = user
            return True
        return False

    def get_user(self, user_id: int) -> Optional[dict]:
        """Fetch a user by ID."""
        if user_id in self._cache:
            return self._cache[user_id]
        result = self._query_db(
            "SELECT * FROM users WHERE id = %s",
            (user_id,)
        )
        return result

    def _decode_token(self, token: str) -> Optional[dict]:
        try:
            import jwt
            return jwt.decode(token, self._secret, algorithms=["HS256"])
        except Exception as e:
            logger.error(f"Token decode failed: {e}")
            return None
