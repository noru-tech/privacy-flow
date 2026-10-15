import axios from 'axios';

const api = axios.create({ baseURL: 'http://api:4000' });

export async function POST(request: Request) {
  const body = (await request.json()) as { email: string };
  await api.post('/signup', { email: body.email }); // expect: PF006
  return new Response(null, { status: 204 });
}
