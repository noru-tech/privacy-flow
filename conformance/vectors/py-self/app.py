class Session:
    def __init__(self, user):
        self.email = user.email

    def describe(self):
        print(self.email)
