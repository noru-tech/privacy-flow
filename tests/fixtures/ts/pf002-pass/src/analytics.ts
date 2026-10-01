import { PostHog } from 'posthog-node';

const posthog = new PostHog(process.env.POSTHOG_KEY ?? '');

export function trackSignup(user: { id: string; email: string }) {
  posthog.capture({ distinctId: user.id, event: 'signup', properties: { email: user.email } });
}
