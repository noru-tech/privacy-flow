function show(input) {
  console.log(input.owner.plan);
  console.log(input.owner);
}
export function f(user) {
  show({ owner: { plan: user.plan, address: user.email } });
}
