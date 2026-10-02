class Box:
    def __init__(self, content):
        self.content = content


def f(user):
    secret = Box(user.email)
    plain = Box("nothing")
    print(plain)
