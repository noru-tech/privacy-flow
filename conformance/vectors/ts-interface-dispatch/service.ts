import type { Mailer } from './types';

export class Signup {
  constructor(private readonly mailer: Mailer) {}

  welcome(user) {
    this.mailer.send(user.email);
  }
}
