import { AppError } from './errors';

export function reject(user: { email: string }) {
  throw new AppError('NOT_ALLOWED', `no access for ${user.email}`);
}
