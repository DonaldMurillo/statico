import type { Post } from '../types/post';

export function getPost(slug: string): Post {
  return { id: 1, title: '', slug, content: '' };
}
