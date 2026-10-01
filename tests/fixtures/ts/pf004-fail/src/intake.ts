import { Analytics } from '@segment/analytics-node';

const analytics = new Analytics({ writeKey: 'key' });

interface Patient { id: string; health_record_id: string; religion: string }

export function admitted(p: Patient) {
  analytics.track({ userId: p.id, event: 'admitted', properties: { record: p.health_record_id } }); // expect: PF004
}
