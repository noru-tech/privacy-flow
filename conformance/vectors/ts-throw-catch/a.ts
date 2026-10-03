export function find(user) {
  try {
    throw new Error(`no account for ${user.email}`);
  } catch (e) {
    console.error(e);
  }
}
