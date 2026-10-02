import { registry } from './registry';

registry.register({
  id: 'audit',
  analyze(value: string) {
    console.log('audit', value); // expect: PF001
  },
});

// An unrelated map: what it holds is not what the registry holds.
const labels = new Map<string, string>();
labels.set('audit', 'Audit');
export const auditLabel = labels.get('audit');
