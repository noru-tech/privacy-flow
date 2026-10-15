import axios from 'axios';

const api = axios.create({ baseURL: 'http://api:4000' });

export async function POST(request: Request) {
  const body = (await request.json()) as { email: string };
  await api.post('/signup', { email: body.email }); // internal: baseURL names the api service
  return new Response(null, { status: 204 });
}
