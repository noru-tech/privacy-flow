import { MailerService } from '@nestjs-modules/mailer';
import nodemailer from '@tryghost/nodemailer';
import { emailSender } from 'wasp/server/email';
import type { User } from './user';

export class InviteMailer {
  constructor(private readonly mailerService: MailerService) {}

  async invite(user: User) {
    await this.mailerService.sendMail({ to: user.email, subject: 'You are invited' }); // expect-flow
  }
}

export async function ghostWelcome(user: User) {
  const transport = nodemailer('SMTP', { host: 'localhost' });
  await transport.sendMail({ to: user.email, subject: 'Welcome' }); // expect-flow
}

export async function waspWelcome(user: User) {
  await emailSender.send({ to: user.email, subject: 'Welcome', text: 'Hello', html: '<p>Hello</p>' }); // expect-flow
}
