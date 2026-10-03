def f(user):
    d = {"a": {"plan": user.plan, "inner": {"email": user.email}}}
    print(d["a"]["plan"])
    print(d["a"]["inner"])
