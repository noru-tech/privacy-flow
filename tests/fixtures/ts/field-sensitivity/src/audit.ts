type User = { id: string; email: string; createdAt: string };

function describe(u: User) {
  return { id: u.id, at: u.createdAt };
}

export function audit(user: User) {
  const d = describe(user);
  console.log('audit', d.id, d.at);
  const copy = { ...user };
  console.log('created', copy.createdAt);
  console.log('who', copy.email); // expect: PF001
}
