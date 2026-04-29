import { posts } from '../../lib/posts';

export default function BlogSlug() {
  return posts.getAll().join(', ');
}
