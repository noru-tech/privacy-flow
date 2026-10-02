function wrap(user) {
  return { data: { inner: user.email, id: user.id } };
}
export function f(user) {
  const r = wrap(user);
  console.log(r.data.id);
  console.log(r.data);
}
