class Transport {
  notify(arg) {
    console.log(arg);
  }
}

export class Channel {
  get transport(): Transport {
    return makeTransport();
  }

  send(user) {
    this.transport.notify(user.email);
  }
}
