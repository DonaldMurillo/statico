import { helpers } from '../utils/helpers';

export const HomeView = {
  name: 'HomeView',
  setup() {
    return helpers.formatDate(new Date());
  },
};
