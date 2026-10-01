import { prisma } from './db';

export async function rename(user: { id: string; email: string }) {
  await prisma.user.update({ where: { id: user.id }, data: { email: user.email } });
}
