function same(x) {
  return x;
}

export function f(user) {
  const a = same(user.email);
  const b = same(user.plan);
  console.log(b);
}
