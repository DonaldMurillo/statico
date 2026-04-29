import { auth } from './src/lib/auth';

export function middleware(request: Request) {
  auth(request);
}
