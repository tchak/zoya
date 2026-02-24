import type { ReactNode } from 'react';

export function Card({
  title,
  children,
}: {
  title: string;
  children: ReactNode;
}) {
  return (
    <div className="bg-white rounded-lg border border-gray-200 p-5">
      <h2 className="text-lg font-semibold text-gray-900 mb-3">{title}</h2>
      {children}
    </div>
  );
}
