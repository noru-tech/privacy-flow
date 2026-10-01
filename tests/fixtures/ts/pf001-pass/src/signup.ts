import pino from 'pino';
import { redact } from './redact';

const logger = pino();

interface Account { id: string; email: string; plan: string }

export function onSignup(account: Account) {
  logger.info({ id: account.id, plan: account.plan });
  logger.info(`welcome ${redact(account.email)}`);
  console.log('signup for', account.email.length, 'chars');
}
