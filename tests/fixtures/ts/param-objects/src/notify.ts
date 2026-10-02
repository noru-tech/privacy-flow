type User = { id: string; name: string; email: string };

function audit(entry: { actor: { id: string; email: string } }) {
  console.log('actor', entry.actor.id);
}

function describe(input: { owner: { id: string; email: string } }) {
  return render(input.owner);
}

function render(owner: { id: string; email: string }) {
  console.log('owner', owner.id);
  return owner.id;
}

function spreadAll(...entries: Array<{ actor: User }>) {
  console.log('first', entries[0].actor.id); // expect: PF001
}

export function run(user: User) {
  audit({ actor: { id: user.id, email: user.email } });
  describe({ owner: { id: user.id, email: user.email } });
  spreadAll({ actor: user });
  const recipients = [{ address: user.email, name: user.name }];
  recipients.forEach((recipient) => console.log('to', recipient.name)); // expect: PF001
}
