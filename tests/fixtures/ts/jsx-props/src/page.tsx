import posthog from 'posthog-js';
import { UserCard } from './components/user-card';

export function Page({ user }: { user: { name: string; email: string } }) {
  return <UserCard user={user} onSelect={(email) => posthog.capture('selected', { email })} />; // expect: PF002
}
