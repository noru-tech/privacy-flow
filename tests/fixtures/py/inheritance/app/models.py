from app.transports import EmailTransport, Transport

TRANSPORTS = {"email": EmailTransport}


class User:
    email: str
    phone_number: str


class Channel:
    kind: str
    email: str

    @property
    def transport(self) -> Transport:
        return TRANSPORTS[self.kind](self)

    def send(self, user: User):
        self.transport.notify(user.phone_number)


class Dispatcher:
    def __init__(self, transport: Transport):
        self.transport = transport

    def dispatch(self, user: User):
        self.transport.notify(user.phone_number)
