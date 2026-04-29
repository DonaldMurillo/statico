export interface Config {
  host: string;
  port: number;
}

export type Status = 'active' | 'inactive';

export interface DeadType {
  x: number;
}
