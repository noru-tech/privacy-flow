import { load } from 'adapter-loader';

export interface Mailer {
  send(to: string): void;
}

export class ConsoleMailer implements Mailer {
  send(to: string) {
    console.log(to); // expect: PF001
  }
}

// The mailer in use is loaded at run time: it may be ConsoleMailer, or anything else.
const mailer: Mailer = load('mailer');

export function welcome(user: { email: string }) {
  mailer.send(user.email); // expect: PFC01
}
