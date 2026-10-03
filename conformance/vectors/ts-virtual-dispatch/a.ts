class Notifier {
  notify(arg) {
    this.deliver(arg);
  }

  deliver(arg) {}
}

class ConsoleNotifier extends Notifier {
  deliver(arg) {
    console.log(arg);
  }
}

export function run(user) {
  new ConsoleNotifier().notify(user.email);
}
