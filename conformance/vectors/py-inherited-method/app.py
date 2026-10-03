class Base:
    def report(self, arg):
        print(arg)


class Child(Base):
    pass


def run(user):
    Child().report(user.email)
