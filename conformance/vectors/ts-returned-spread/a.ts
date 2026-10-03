function withOwner(record, user) {
  return { ...record, owner: { plan: user.plan, address: user.email } };
}
export function f(record, user) {
  const r = withOwner(record, user);
  console.log(r.owner.plan);
  console.log(r.owner);
}
