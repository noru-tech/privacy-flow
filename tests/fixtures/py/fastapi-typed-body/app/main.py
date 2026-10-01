import logging

from fastapi import FastAPI

from app.schemas import SignupIn

app = FastAPI()
logger = logging.getLogger("app")


@app.post("/signup")
def signup(body: SignupIn, ref: str | None = None):
    logger.info("plan %s", body.plan)
    logger.info("email %s", body.email)  # expect: PF001
    return {"ok": True}
