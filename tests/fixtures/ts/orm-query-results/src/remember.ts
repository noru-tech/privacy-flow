declare global {
  var __remember: Map<string, unknown>;
}

export function remember<T>(name: string, make: () => T): T {
  const thusly = globalThis;
  if (!thusly.__remember) {
    thusly.__remember = new Map();
  }
  if (!thusly.__remember.has(name)) {
    thusly.__remember.set(name, make());
  }
  return thusly.__remember.get(name);
}
