import twilio from 'twilio';

export class Notifier {
  private client = twilio('sid', 'token');

  constructor(private readonly from: string) {}

  async sms(phone: string, text: string) {
    await this.client.messages.create({ from: this.from, to: phone, body: text }); // expect: PF002
  }
}

export async function remind(user: { phone_number: string }) {
  const n = new Notifier('+15550000');
  await n.sms(user.phone_number, 'Your appointment is tomorrow');
}
