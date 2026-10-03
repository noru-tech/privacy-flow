import logging

logger = logging.getLogger(__name__)


class Transport:
    def __init__(self, channel):
        self.channel = channel

    def notify(self, message):
        raise NotImplementedError

    def audit(self, entry):
        logger.info(entry)  # expect: PF001


class EmailTransport(Transport):
    def notify(self, message):
        logger.warning("to %s: %s", self.channel.email, message)  # expect: PF001


class SmsTransport(Transport):
    def notify(self, message):
        self.audit(message)

    def audit(self, entry):
        super().audit(entry)
