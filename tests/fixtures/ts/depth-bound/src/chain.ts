function a(x: string) { return b(x); } // expect: PFC01
function b(x: string) { return c(x); }
function c(x: string) { console.log(x); return x; }

export function start(user: { email: string }) {
  a(user.email); // expect: PFC01
}
