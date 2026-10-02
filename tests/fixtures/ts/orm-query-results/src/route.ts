import { prisma } from './db';

export async function invite(user: { email: string }) {
  const found = await prisma.user.findFirst({ where: { email: user.email } });
  console.log('invited', found.id);
  const created = await prisma.invite.create({ data: { email: user.email } });
  console.log('invite', created.status);
  console.log('to', created.email); // expect: PF001
}
