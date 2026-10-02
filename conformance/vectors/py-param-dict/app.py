def show(box):
    print(box["owner"]["id"])
    print(box["owner"])


def f(user):
    show({"owner": {"id": user.id, "address": user.email}})
