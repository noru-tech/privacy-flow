export function f(user) {
  const o = { a: { id: user.id, inner: { email: user.email } } };
  console.log(o.a.id);
  console.log(o.a.inner);
}
