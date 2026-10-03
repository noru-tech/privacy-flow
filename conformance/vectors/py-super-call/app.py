class Base:
    def report(self, arg):
        print(arg)


class Child(Base):
    def report(self, arg):
        super().report(arg)


def run(user):
    Child().report(user.email)
