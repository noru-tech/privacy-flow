export async function callPartner(cfg: { endpoint: string; api_key: string }, user: { email: string }) {
  await fetch(cfg.endpoint, { // expect: PF006
    method: 'POST',
    headers: { Authorization: `Bearer ${cfg.api_key}` },
    body: JSON.stringify({ email: user.email }),
  });
}
