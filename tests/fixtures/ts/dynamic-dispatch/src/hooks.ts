type Hook = (email: string) => void;

export function run(hooks: Hook[], user: { email: string }) {
  for (const hook of hooks) {
    hook(user.email); // expect: PFC01
  }
}
