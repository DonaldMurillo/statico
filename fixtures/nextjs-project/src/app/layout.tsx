import { cn } from '../lib/utils';

export default function RootLayout({ children }: { children: React.ReactNode }) {
  return <html><body className={cn('min-h-screen')}>{children}</body></html>;
}
