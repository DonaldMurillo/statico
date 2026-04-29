import { useAuth } from '../hooks/useAuth';

export function Header() {
  const auth = useAuth();
  return <header>Header {auth.user}</header>;
}
