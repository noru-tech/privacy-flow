import logging

from app.errors import AppError

log = logging.getLogger(__name__)


def on_failure():
    error = AppError("NETWORK", "request failed")
    log.error("failed: %s", error)
