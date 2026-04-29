import { env } from '../config/env';
import { CONSTANTS } from './constants';

export const db = {
  query: async (sql: string) => {
    console.log(env.dbUrl, CONSTANTS.MAX_CONNECTIONS);
    return [];
  }
};
