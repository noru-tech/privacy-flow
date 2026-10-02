export function f(user) {
  const email = user.email;
  const later = () => console.log(email);
  later();
}
