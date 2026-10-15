import { PostHog } from 'posthog-node';

const posthog = new PostHog('phc_key');

export function track(email: string) {
  posthog.capture({ distinctId: email, event: 'signup' }); // expect: PF002
}
