import httpx

CRM = "https://api.crm.acme.example/v2"


def notify(user):
    httpx.post(CRM + "/contacts", json={"email": user.email})
