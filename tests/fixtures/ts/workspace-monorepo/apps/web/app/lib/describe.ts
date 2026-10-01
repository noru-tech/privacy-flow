export function describe(u: { name: string; phone_number: string }) {
  return `${u.name} <${u.phone_number}>`;
}
