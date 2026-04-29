import React from 'react';

export function Button({
  children,
  variant = 'primary',
}: {
  children: React.ReactNode;
  variant?: 'primary' | 'secondary';
}): React.ReactElement {
  const className = variant === 'primary' ? 'btn-primary' : 'btn-secondary';
  return <button className={className}>{children}</button>;
}
