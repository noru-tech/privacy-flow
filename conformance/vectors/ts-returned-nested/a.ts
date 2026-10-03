function wrap(user) {
  return { data: { inner: user.email, plan: user.plan } };
}
export function f(user) {
  const r = wrap(user);
  console.log(r.data.plan);
  console.log(r.data);
}
