class Notifier:
    def notify(self, arg):
        self.deliver(arg)

    def deliver(self, arg):
        raise NotImplementedError


class PrintNotifier(Notifier):
    def deliver(self, arg):
        print(arg)


def run(user):
    PrintNotifier().notify(user.email)
