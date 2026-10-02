import { getEnvelope, getEnvelopeWithDetails } from './envelopes';

export async function show(id: string) {
  const envelope = await getEnvelope(id);
  console.log('owner', envelope.user.id);
  console.log('owner', envelope.user); // expect: PF001
}

export async function details(id: string) {
  const document = await getEnvelopeWithDetails(id).catch(() => null);
  console.log('document', document.envelopeId);
  console.log('owner', document.user.id);
}

export async function later(id: string) {
  const envelope = await getEnvelope(id);
  const describe = () => `${envelope.user.id}`;
  console.log('later', describe());
}
