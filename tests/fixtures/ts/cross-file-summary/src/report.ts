import { line, label } from './lib/format';
import { logger } from './lib/logger';

export function report(c: { id: string; name: string; phone_number: string }) {
  logger.info(label(c.id));
  logger.info(line(c.name, c.phone_number)); // expect: PF001
}
