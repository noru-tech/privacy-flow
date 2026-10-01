from flask import Flask, request
import structlog

app = Flask(__name__)
log = structlog.get_logger()


@app.post("/login")
def login():
    token = request.headers.get("x-session")
    log.info("login", api_key=request.form["api_key"])  # expect: PF005
    return {"ok": True}
