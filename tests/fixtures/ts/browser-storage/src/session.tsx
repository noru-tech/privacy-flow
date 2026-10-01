export function remember(profile: { email: string; theme: string }) {
  localStorage.setItem('theme', profile.theme);
  localStorage.setItem('email', profile.email); // expect-flow
  document.cookie = `email=${profile.email}`; // expect-flow
}
