import React from 'react';

export function Button({ label }: { label: string }): React.ReactElement {
  return <button type="button">{label}</button>;
}
