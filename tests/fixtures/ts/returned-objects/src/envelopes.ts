import { PrismaClient } from '@prisma/client';

const prisma = new PrismaClient();

// A record from the database, returned with an owner object of its own.
export async function getEnvelope(id: string) {
  const envelope = await prisma.envelope.findFirstOrThrow({ where: { id }, include: { user: true } });
  return {
    ...envelope,
    user: { id: envelope.user.id, address: envelope.user.email },
  };
}

// A second layer that spreads the first.
export async function getEnvelopeWithDetails(id: string) {
  const envelope = await getEnvelope(id);
  return { ...envelope, envelopeId: envelope.id };
}
