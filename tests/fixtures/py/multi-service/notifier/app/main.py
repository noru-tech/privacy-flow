import logging

from fastapi import FastAPI
from pydantic import BaseModel

log = logging.getLogger(__name__)
app = FastAPI()


class Notify(BaseModel):
    email: str


@app.post("/notify")
def notify(body: Notify):
    log.info("notify %s", body.email)  # expect: PF001
    return {"ok": True}
