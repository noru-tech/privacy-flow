import httpx
from django.http import HttpResponse

notifier = httpx.Client(base_url="http://notifier:8000")


def signup(request):
    email = request.POST["email"]
    notifier.post("/notify", json={"email": email})  # internal: base_url names the notifier service
    return HttpResponse(status=201)
