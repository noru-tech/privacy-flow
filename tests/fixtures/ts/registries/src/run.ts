import { registry, Scheduler } from './registry';

export function direct(user: { email: string }) {
  registry.get('audit')?.analyze(user.email);
}

export function scheduled(user: { email: string }) {
  const scheduler = new Scheduler();
  registry.each(scheduler);
  scheduler.run('audit', user.email);
}
