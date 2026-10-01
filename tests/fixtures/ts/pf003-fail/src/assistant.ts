import OpenAI from 'openai';

const openai = new OpenAI();

type Customer = { name: string; email: string; tier: string };

export async function draftReply(customer: Customer, question: string) {
  const prompt = `Customer ${customer.email} (${customer.tier}) asks: ${question}`;
  return openai.chat.completions.create({ // expect: PF002 PF003
    model: 'gpt-4o',
    messages: [{ role: 'user', content: prompt }],
  });
}
