export function f(user) {
  const o = { a: { plan: user.plan, inner: { email: user.email } } };
  console.log(o.a.plan);
  console.log(o.a.inner);
}
