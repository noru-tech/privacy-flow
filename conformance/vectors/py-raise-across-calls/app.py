def check(user):
    if not user.active:
        raise ValueError(user.email)


def load(user):
    check(user)


def run(user):
    try:
        load(user)
    except ValueError as e:
        print(e)
