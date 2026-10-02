import pino from 'pino';
import { PostHog } from 'posthog-node';
import OpenAI from 'openai';
import { format } from './format';

const logger = pino();
const posthog = new PostHog('key');
const openai = new OpenAI();

interface User { id: string; email: string; phone_number: string; plan: string }

export async function signup(user: User) {
  logger.info({ id: user.id });
  logger.info(`new user ${user.email}`);
  posthog.capture({ distinctId: user.id, event: 'signup', properties: { email: user.email } });
  const prompt = format(user);
  await openai.chat.completions.create({ model: 'gpt-4o', messages: [{ role: 'user', content: prompt }] });
}
