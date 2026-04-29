import { Header } from '../components/Header';
import { Footer } from '../components/Footer';
import { ThemeProvider } from '../providers/ThemeProvider';
import { AuthProvider } from '../providers/AuthProvider';

export default function Layout({ children }: { children: React.ReactNode }) {
  return (
    <ThemeProvider>
      <AuthProvider>
        <Header />
        {children}
        <Footer />
      </AuthProvider>
    </ThemeProvider>
  );
}
