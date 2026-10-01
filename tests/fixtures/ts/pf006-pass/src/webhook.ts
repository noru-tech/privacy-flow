const CRM = 'https://api.crm.acme.example/v2';

export async function notify(user: { id: string; email: string }) {
  await fetch(`${CRM}/contacts`, { method: 'POST', body: JSON.stringify({ email: user.email }) });
  await fetch('/api/audit', { method: 'POST', body: JSON.stringify({ id: user.id, email: user.email }) });
}
