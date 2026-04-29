import { Layout } from './components/Layout';
import { utils } from './lib/utils';

export default function Root() {
  return Layout.render(utils.format());
}
