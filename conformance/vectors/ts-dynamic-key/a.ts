export function f(user, key) {
  const o = { email: user.email, plan: user.plan };
  console.log(o[key]);
}
