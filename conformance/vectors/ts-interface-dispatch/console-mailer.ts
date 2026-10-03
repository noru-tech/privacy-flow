import type { Mailer } from './types';

export class ConsoleMailer implements Mailer {
  send(to: string) {
    console.log(to);
  }
}
