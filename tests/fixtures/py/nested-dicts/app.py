import logging

logger = logging.getLogger(__name__)

SETTINGS = {"audit": {"target": "stdout", "level": "info"}}


def record(account):
    entry = {"meta": {"id": account.id, "contact": {"address": account.email}}}
    logger.info("entry %s", entry["meta"]["id"])
    logger.info("contact %s", entry["meta"]["contact"])  # expect: PF001


def configure(account):
    SETTINGS["audit"]["target"] = account.email
    logger.info("level %s", SETTINGS["audit"]["level"])


def report():
    logger.info("target %s", SETTINGS["audit"]["target"])  # expect: PF001
