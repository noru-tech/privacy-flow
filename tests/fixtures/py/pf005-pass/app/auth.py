import logging

import bcrypt

logger = logging.getLogger(__name__)


def register(user, password):
    hashed = bcrypt.hashpw(password.encode(), bcrypt.gensalt())
    logger.info("stored hash of length %d for %s", len(hashed), user.id)
