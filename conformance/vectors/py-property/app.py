class Transport:
    def notify(self, arg):
        print(arg)


class Channel:
    @property
    def transport(self) -> Transport:
        return make_transport()

    def send(self, user):
        self.transport.notify(user.email)
