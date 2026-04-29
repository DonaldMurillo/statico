import { check } from './conditional';
import('./lazy');
import('./feature').then(m => m.run());
console.log(check);
