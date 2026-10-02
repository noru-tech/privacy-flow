export function f(user, key) {
  const o = { email: user.email, id: user.id };
  console.log(o[key]);
}
