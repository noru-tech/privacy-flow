export function format(u: { phone_number: string; plan: string }) {
  return `Call ${u.phone_number} about ${u.plan}`;
}
