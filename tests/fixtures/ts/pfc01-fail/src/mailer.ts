import { deliver } from 'acme-mailer';

export function welcome(user: { email: string }) {
  deliver(user.email, 'Welcome!'); // expect: PFC01
}
