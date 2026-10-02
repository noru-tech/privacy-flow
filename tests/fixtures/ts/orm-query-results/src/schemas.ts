import { z } from 'zod';

// An unrelated map elsewhere in the program: what it holds is not what `remember` returns.
const schemas = new Map();
schemas.set('signup', z.object({ email: z.string() }));

export const signupSchema = schemas.get('signup');
