def wrap(user):
    return {"data": {"inner": user.email, "plan": user.plan}}


def f(user):
    r = wrap(user)
    print(r["data"]["plan"])
    print(r["data"])
