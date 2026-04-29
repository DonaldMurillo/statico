import React from 'react';
import { Button } from '@mono/ui';
import { utils } from '@mono/shared';

export function Page(): React.ReactElement {
  return (
    <div>
      <h1>Welcome</h1>
      <Button label={`Click ${utils.id()}`} />
    </div>
  );
}
