import logging

logger = logging.getLogger(__name__)


class Address:
    street: str
    city: str
    state: str


class ChatContext:
    state: dict
    llm: object


def typed_address(address: Address):
    logger.info(address.state)  # expect: PF001


def typed_context(context: ChatContext):
    logger.info(context.state)


def untyped(run, billing_address, order):
    logger.info(run.state)
    logger.info(billing_address["state"])  # expect: PF001
    logger.info(order.shipping_address.state)  # expect: PF001
    logger.info(order.tax_breakdown[0]["state"])


def update_address(street, city, state, zip_code):
    logger.info(state)  # expect: PF001


def set_state(state):
    logger.info(state)
