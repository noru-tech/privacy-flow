import logging

from app.privacy import mask

logger = logging.getLogger(__name__)


def on_signup(account):
    logger.info("signup %s", account.id)
    logger.info("welcome %s", mask(account.email))
    logger.info("email length %d", len(account.email))
