class Base {
  report(arg) {
    console.log(arg);
  }
}

class Child extends Base {
  report(arg) {
    super.report(arg);
  }
}

export function run(user) {
  new Child().report(user.email);
}
