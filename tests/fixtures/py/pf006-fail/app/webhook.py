import os

import httpx


def notify(user):
    httpx.post(os.environ["WEBHOOK_URL"], json={"email": user.email})  # expect: PF006
