import { PrismaClient } from '@prisma/client';

declare global {
  var __cache: Map<string, unknown>;
}

function remember<T>(name: string, make: () => T): T {
  globalThis.__cache ??= new Map();
  if (!globalThis.__cache.has(name)) {
    globalThis.__cache.set(name, make());
  }
  return globalThis.__cache.get(name) as T;
}

export const prisma = remember('prisma', () => new PrismaClient());
