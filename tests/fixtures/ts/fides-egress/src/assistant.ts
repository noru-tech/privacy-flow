import { generateText } from 'ai';

type Customer = { name: string; email: string };

export async function suggest(model: any, customer: Customer) {
  return generateText({ model, prompt: `Write to ${customer.email}` }); // expect: PF003
}
