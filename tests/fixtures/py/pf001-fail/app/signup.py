import logging

logger = logging.getLogger(__name__)


def on_signup(account):
    logger.info("signup %s", account.id)
    logger.info("welcome %s", account.email)  # expect: PF001
    print(f"phone: {account.phone_number}")  # expect: PF001
