export function Badge({ label, count }: { label: string; count: number }) {
  return (
    <span className="inline-flex items-center gap-1.5 px-3 py-1 rounded-full text-sm font-medium bg-white border border-gray-200 text-gray-700">
      {label}
      <span className="bg-gray-100 text-gray-600 px-1.5 py-0.5 rounded-full text-xs font-semibold">
        {count}
      </span>
    </span>
  );
}
