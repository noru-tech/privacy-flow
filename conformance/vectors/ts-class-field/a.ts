export class Session {
  email: string;

  constructor(user) {
    this.email = user.email;
  }

  describe() {
    console.log(this.email);
  }
}
