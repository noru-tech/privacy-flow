def show(box):
    print(box["owner"]["plan"])
    print(box["owner"])


def f(user):
    show({"owner": {"plan": user.plan, "address": user.email}})
