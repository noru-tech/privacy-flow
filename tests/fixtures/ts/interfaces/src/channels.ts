import pino from 'pino';

const log = pino();

export interface Notifier {
  notify(message: string): Promise<void>;
}

export interface Mailer extends Notifier {
  bounce(address: string): void;
}

export class LogMailer implements Mailer {
  async notify(message: string) {
    log.info(message); // expect: PF001
  }

  bounce(address: string) {
    log.warn(address); // expect: PF001
  }
}

export abstract class Pager implements Notifier {
  abstract notify(message: string): Promise<void>;
}

export class ConsolePager extends Pager {
  async notify(message: string) {
    console.log(message); // expect: PF001
  }
}
