def show(a=None, b=None):
    print(b)


def f(user):
    show(a=user.email, b=user.plan)
    show(b=user.phone_number)
