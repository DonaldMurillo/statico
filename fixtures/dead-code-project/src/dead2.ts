// Dead: only imported by dead1 which is itself dead.
import { dead1 } from './dead1';
export function dead2() { return dead1(); }
