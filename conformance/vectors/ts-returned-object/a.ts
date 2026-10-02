function build(user) {
  return { id: user.id, slot: user.email };
}
export function f(user) {
  const o = build(user);
  console.log(o.id);
  console.log(o.slot);
}
