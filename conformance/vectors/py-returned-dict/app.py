def wrap(user):
    return {"data": {"inner": user.email, "id": user.id}}


def f(user):
    r = wrap(user)
    print(r["data"]["id"])
    print(r["data"])
