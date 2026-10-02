from flask import Flask, request

app = Flask(__name__)


@app.post("/signup")
def signup():
    print(request.form)
    return "ok"
