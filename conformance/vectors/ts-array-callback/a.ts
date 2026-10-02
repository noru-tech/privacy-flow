export function f(users) {
  const emails = users.map((u) => u.email);
  emails.forEach((e) => console.log(e));
}
