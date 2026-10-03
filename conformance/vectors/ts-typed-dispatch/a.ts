class Notifier {
  deliver(arg) {}
}

class ConsoleNotifier extends Notifier {
  deliver(arg) {
    console.log(arg);
  }
}

export class Outbox {
  constructor(private readonly notifier: Notifier) {}

  send(user) {
    this.notifier.deliver(user.email);
  }
}
