import type { Mailer, Notifier } from './channels';

export interface Customer {
  id: string;
  email: string;
}

export class Alerts {
  constructor(
    private readonly notifiers: Notifier,
    private readonly mailer: Mailer,
  ) {}

  async signup(customer: Customer) {
    await this.notifiers.notify(`welcome ${customer.email}`);
    this.mailer.bounce(customer.email);
  }
}
