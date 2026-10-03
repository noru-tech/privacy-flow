import { PrismaClient } from '@prisma/client';
const prisma = new PrismaClient();

export function direct(user) {
  try {
    throw new Error(`no such user ${user.email}`);
  } catch (e) {
    console.error(e); // expect: PF001
  }
}

function check(user) {
  if (!user.ok) throw new Error(user.phone_number);
}

function outer(user) {
  check(user);
}

export function nested(user) {
  try {
    outer(user);
  } catch (err) {
    console.warn(err); // expect: PF001
  }
}

export async function echoed(user) {
  try {
    await prisma.user.create({ data: { email: user.email } });
  } catch (error) {
    console.error('create failed', error); // expect: PF001
  }
}

function local(x) {
  return x.length;
}

export function notEchoed(user) {
  try {
    local(user.password);
  } catch (e) {
    console.log(e);
  }
}
