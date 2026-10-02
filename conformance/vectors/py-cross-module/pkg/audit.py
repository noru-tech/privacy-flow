import logging

log = logging.getLogger("audit")


def record(entry):
    log.warning("audit %s", entry)
