import { HeaderComponent } from '../src/components/header.component';

export default {
  title: 'Components/Header',
  component: HeaderComponent,
};

export const Default = () => ({
  component: HeaderComponent,
  props: { title: 'Storybook Header' },
});
