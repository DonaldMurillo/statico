import React from 'react';
import { Button } from '@repo/ui';
import { env } from '@repo/config';

export function Page(): React.ReactElement {
  return (
    <div>
      <h1>{env.VERSION}</h1>
      <Button variant="primary">Get Started</Button>
    </div>
  );
}
