export function f(user) {
  const o = { a: { slot: 'x' } };
  const p = o;
  p.a = { slot: user.email };
  console.log(o.a.slot);
}
