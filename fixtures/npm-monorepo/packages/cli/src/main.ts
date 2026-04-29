import { getUsers } from '@mono/core';
import { handleCommand } from './commands';

const users = getUsers();
handleCommand('list', users);
