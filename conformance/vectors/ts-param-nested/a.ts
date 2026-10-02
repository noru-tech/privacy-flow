function show(input) {
  console.log(input.owner.id);
  console.log(input.owner);
}
export function f(user) {
  show({ owner: { id: user.id, address: user.email } });
}
