function audit(entry) {
  console.log('audit', entry);
}

export function f(user) {
  audit(user.email);
}
