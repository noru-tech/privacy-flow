import { AppError } from './errors';

export function onFailure() {
  const error = new AppError('NETWORK', 'request failed');
  console.error(error);
  console.error(error.describe()); // expect: PF001
}
