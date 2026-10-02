export function f(user) {
  const o = { email: user.email, plan: 'pro' };
  const c = { ...o };
  console.log(c.plan);
  console.log(c.email);
}
