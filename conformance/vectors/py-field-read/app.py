import logging

log = logging.getLogger(__name__)


def f(user):
    log.info("user %s", user.email)
