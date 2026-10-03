import logging

import stripe

logger = logging.getLogger(__name__)


class User:
    id: str
    email: str


class Order:
    id: str
    total: int


def checkout(user: User, order: Order, team):
    logger.info("checkout for user %s", user.id)  # expect-flow
    logger.info("order %s", order.id)
    logger.info("team %s", team.id)
    stripe.Customer.create(metadata={"user_id": user.id})  # expect: PF002


def refund(customer_id: str, amount: int):
    logger.info("refund %s for %s", amount, customer_id)  # expect-flow
