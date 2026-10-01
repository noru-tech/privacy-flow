import { PrismaClient } from '@prisma/client';

const prisma = new PrismaClient();

interface Patient { id: string; health_record_id: string; religion: string }

export async function admitted(p: Patient) {
  await prisma.admission.create({ data: { patientId: p.id, record: p.health_record_id } });
}
