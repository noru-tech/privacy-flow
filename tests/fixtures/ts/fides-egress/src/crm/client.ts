export async function sync(user: { phone: string }) {
  const url = process.env.CRM_URL as string;
  await fetch(url, { method: 'POST', body: JSON.stringify({ phone: user.phone }) }); // expect: PF006
}
