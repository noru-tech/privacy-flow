import type { Logger } from 'pino';
import type { Transporter } from 'nodemailer';

export class SignupService {
  constructor(private readonly logger: Logger, private readonly mail: Transporter) {}

  async welcome(user: { id: string; email: string }) {
    this.logger.info({ userId: user.id });
    this.logger.debug(`welcome mail for ${user.email}`); // expect: PF001
    await this.mail.sendMail({ to: user.email, subject: 'Welcome' }); // expect-flow
  }
}
