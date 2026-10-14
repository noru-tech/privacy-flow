export async function charge(user: { email: string }) {
  const url = process.env.BILLING_URL as string;
  await fetch(url, { method: 'POST', body: JSON.stringify({ email: user.email }) }); // expect: PF006
}
