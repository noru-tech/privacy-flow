export async function notify(user: { id: string; email: string }) {
  const url = process.env.WEBHOOK_URL as string;
  await fetch(url, { method: 'POST', body: JSON.stringify({ email: user.email }) }); // expect: PF006
}
