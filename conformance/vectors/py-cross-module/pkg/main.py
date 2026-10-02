from pkg.audit import record


def f(user):
    record(user.phone_number)
