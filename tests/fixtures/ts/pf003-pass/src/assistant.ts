import OpenAI from 'openai';

const openai = new OpenAI();

type Customer = { name: string; email: string; tier: string };

export async function draftReply(customer: Customer, question: string) {
  const prompt = `A ${customer.tier} customer asks: ${question}`;
  return openai.chat.completions.create({
    model: 'gpt-4o',
    messages: [{ role: 'user', content: prompt }],
  });
}
