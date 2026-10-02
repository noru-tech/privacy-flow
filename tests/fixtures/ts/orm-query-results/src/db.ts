import { PrismaClient } from '@prisma/client';

import { remember } from './remember';

export const prisma = remember('prisma', () => new PrismaClient());
