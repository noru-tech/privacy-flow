import { logger } from '@acme/logging';
import { describe } from '~/lib/describe';

export function show(u: { name: string; phone_number: string }) {
  logger.info(describe(u)); // expect: PF001
}
