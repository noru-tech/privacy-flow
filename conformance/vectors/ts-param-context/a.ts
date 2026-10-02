function pick(o) {
  console.log(o.v);
}
export function f(user) {
  pick({ v: user.id, w: user.email });
  pick({ v: user.email });
}
