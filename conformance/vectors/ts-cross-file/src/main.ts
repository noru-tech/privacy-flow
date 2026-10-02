import { record } from './log';

export function f(user) {
  record(`user ${user.phone_number}`);
}
