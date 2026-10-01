import sentry_sdk
from django.http import JsonResponse


def checkout(request):
    sentry_sdk.set_user({"email": request.POST.get("email")})  # expect: PF002
    return JsonResponse({"ok": True})
