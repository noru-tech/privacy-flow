function box(v) {
  return { inner: { v } };
}
export function f(user) {
  const a = box(user.email);
  const b = box(user.plan);
  console.log(b.inner.v);
  console.log(a.inner.v);
}
