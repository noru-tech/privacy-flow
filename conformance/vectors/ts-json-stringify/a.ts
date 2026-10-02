export function f(user) {
  const body = JSON.stringify({ email: user.email });
  console.info(body);
}
