class Base:
    def __init__(self, user):
        self.saved = user.email


class Child(Base):
    def describe(self):
        print(self.saved)
