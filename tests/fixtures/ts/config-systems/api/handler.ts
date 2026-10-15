import * as Sentry from '@sentry/node';

export function onError(user: { email: string }, err: Error) {
  Sentry.captureException(err, { user: { email: user.email } }); // expect: PF002
}
