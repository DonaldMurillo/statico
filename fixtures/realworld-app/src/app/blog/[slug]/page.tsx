import { Card } from '../../../components/Card';
import { getPost } from '../../../services/post.service';

export default function BlogPost({ params }: { params: { slug: string } }) {
  const post = getPost(params.slug);
  return <Card title={post.title} />;
}
