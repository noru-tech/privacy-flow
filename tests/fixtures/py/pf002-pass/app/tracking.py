import posthog


def track_signup(user):
    posthog.capture(user.id, "signup", {"email": user.email})
