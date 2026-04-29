import { Header } from '../components/Header';
import { Footer } from '../components/Footer';

export default function Index() {
  return Header.render() + Footer.render();
}
