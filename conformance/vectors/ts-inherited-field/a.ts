class Base {
  constructor(user) {
    this.saved = user.email;
  }
}

export class Child extends Base {
  describe() {
    console.log(this.saved);
  }
}
