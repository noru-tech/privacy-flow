export function f(user) {
  const a = `hello ${user.email}`;
  const b = 'phone: ' + user.phone_number;
  console.log(a, b);
}
