import logging

log = logging.getLogger(__name__)


class Session:
    def __init__(self, user):
        self.email = user.email
        self.id = user.id

    def describe(self):
        log.info("session %s", self.id)
        log.warning("session for %s", self.email)  # expect: PF001
