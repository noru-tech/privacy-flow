class Transport:
    def notify(self, arg):
        raise NotImplementedError


class PrintTransport(Transport):
    def notify(self, arg):
        print(arg)


class Channel:
    def __init__(self, transport: Transport):
        self.transport = transport

    def send(self, user):
        self.transport.notify(user.email)
