import logging

from fastapi import FastAPI
from flask import Flask, request

app = Flask(__name__)
api = FastAPI()
logger = logging.getLogger(__name__)


@app.route("/products")
def products():
    product_id = request.args.get("product_id")
    logger.info("product %s", product_id)
    note = request.args.get("note")
    logger.info("note %s", note)  # expect: PF001
    return "ok"


@api.get("/items/{item_id}")
def get_item(item_id: int, catalog_item_ids: list[int], customer_id: str, q: str):
    logger.info("item %s", item_id)
    logger.info("items %s", catalog_item_ids)
    logger.info("customer %s", customer_id)  # expect-flow
    logger.info("query %s", q)  # expect: PF001
