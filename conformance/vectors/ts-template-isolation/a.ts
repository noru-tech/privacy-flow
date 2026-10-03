class Base {
  run(arg) {
    this.handle(arg);
  }

  handle(arg) {}
}

class Logs extends Base {
  handle(arg) {
    console.log(arg);
  }
}

class Quiet extends Base {
  handle(arg) {}
}

export function quiet(user) {
  new Quiet().run(user.email);
}

export function logged(user) {
  new Logs().run(user.phone_number);
}
