export function f(user) {
  const ok = user.email.includes('@');
  console.log(ok, user.email.length);
}
