import { PostHog } from 'posthog-node';
import pino from 'pino';

const log = pino();

export interface User {
  id: string;
  email: string;
}

class Notifier {
  audit(entry) {
    log.info(entry); // expect: PF001
  }

  notify(entry) {
    this.deliver(entry); // expect: PFC01
  }

  deliver(entry) {}
}

class LogNotifier extends Notifier {
  deliver(entry) {
    log.warn(entry); // expect: PF001
  }
}

export class Outbox {
  constructor(private readonly notifier: Notifier) {}

  send(user: User) {
    this.notifier.deliver(user.email);
  }
}

class QuietNotifier extends Notifier {
  audit(entry) {
    super.audit(entry);
  }
}

export function signup(user: User) {
  new LogNotifier().audit(user.email);
  new LogNotifier().notify(user.email);
  new QuietNotifier().audit(user.email);
}

class Analytics extends PostHog {}

export function track(user: User) {
  new Analytics('key').capture({ distinctId: user.id, event: 'signup', properties: { email: user.email } }); // expect: PF002
}

class Recipient {
  protected target = '';
}

class Addressed extends Recipient {
  address(user: User) {
    this.target = user.email;
  }
}

export class Envelope extends Addressed {
  print() {
    log.debug(this.target); // expect: PF001
  }
}

class Mailer {
  send(to) {
    log.error(to); // expect: PF001
  }
}

export class Registry {
  get mailer(): Mailer {
    return (globalThis as any).mailer;
  }

  welcome(user: User) {
    this.mailer.send(user.email);
  }
}
