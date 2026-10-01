from acme_mailer import deliver


def welcome(user):
    deliver(user.email, "Welcome!")  # expect: PFC01
