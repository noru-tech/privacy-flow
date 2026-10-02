def f(user):
    d = {"a": {"id": user.id, "inner": {"email": user.email}}}
    print(d["a"]["id"])
    print(d["a"]["inner"])
