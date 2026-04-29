import React from 'react';

export function Input({ placeholder }: { placeholder: string }): React.ReactElement {
  return <input type="text" placeholder={placeholder} />;
}
