import { env } from '../config/env';
import { CONSTANTS } from './constants';

export function auth(request: Request) {
  console.log(env.jwtSecret, CONSTANTS.TIMEOUT);
  return { user: null };
}
