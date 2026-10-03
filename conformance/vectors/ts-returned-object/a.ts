function build(user) {
  return { plan: user.plan, slot: user.email };
}
export function f(user) {
  const o = build(user);
  console.log(o.plan);
  console.log(o.slot);
}
