class Base {
  report(arg) {
    console.log(arg);
  }
}

class Child extends Base {}

export function run(user) {
  new Child().report(user.email);
}
