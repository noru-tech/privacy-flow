import logging

import requests

logger = logging.getLogger(__name__)


class User:
    email: str
    phone_number: str
    active: bool


def check(user: User):
    if not user.active:
        raise ValueError(f"inactive: {user.email}")


def load(user: User):
    check(user)


def run(user: User):
    try:
        load(user)
    except ValueError as e:
        logger.warning("refused: %s", e)  # expect: PF001


def sync(user: User):
    try:
        requests.post("https://billing.example.com/customers", json={"phone": user.phone_number})  # expect: PF002
    except requests.RequestException as exc:
        logger.error("billing sync failed: %s", exc)  # expect: PF001


def count(name):
    return len(name)


def local(user: User):
    try:
        count(user.email)
    except TypeError as e:
        logger.info("bad input: %s", e)
