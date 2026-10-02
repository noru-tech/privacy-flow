function withOwner(record, user) {
  return { ...record, owner: { id: user.id, address: user.email } };
}
export function f(record, user) {
  const r = withOwner(record, user);
  console.log(r.owner.id);
  console.log(r.owner);
}
