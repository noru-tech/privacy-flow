import pino from 'pino';

const logger = pino();

interface Account { id: string; email: string; plan: string }

export function onSignup(account: Account) {
  logger.info({ id: account.id, plan: account.plan });
  logger.info(`welcome ${account.email}`); // expect: PF001
  console.log('signup', account); // expect: PF001
}
