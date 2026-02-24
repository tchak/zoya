export function ModuleHeader({ module }: { module: string }) {
  if (!module) return null;
  return (
    <div className="text-xs font-medium text-gray-400 mb-1.5">{module}</div>
  );
}
