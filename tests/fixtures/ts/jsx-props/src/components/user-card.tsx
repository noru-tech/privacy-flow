type Props = { user: { name: string; email: string }; onSelect: (email: string) => void };

export function UserCard({ user, onSelect }: Props) {
  return <button onClick={() => onSelect(user.email)}>{user.name}</button>;
}
