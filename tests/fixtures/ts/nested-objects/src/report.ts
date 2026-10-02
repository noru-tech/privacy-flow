type User = { id: string; email: string; plan: string };

const settings = { audit: { target: 'stdout', level: 'info' } };

export function record(user: User) {
  const entry = { meta: { id: user.id, contact: { address: user.email } } };
  console.log('entry', entry.meta.id);
  console.log('contact', entry.meta.contact); // expect: PF001
  const meta = entry.meta;
  console.log('meta id', meta.id);
}

export function overwrite(user: User) {
  const state = { last: { plan: user.plan } };
  const alias = state;
  alias.last = { plan: user.plan, contact: user.email };
  console.log('last', state.last.contact); // expect: PF001
  console.log('plan', state.last.plan);
}

export function later(user: User) {
  const box = { inner: { id: user.id, mail: user.email } };
  return () => {
    console.log('later', box.inner.id);
    console.log('later', box.inner.mail); // expect: PF001
  };
}

export function configure(user: User) {
  settings.audit.target = user.email;
  console.log('level', settings.audit.level);
}

export function report() {
  console.log('target', settings.audit.target); // expect: PF001
}
