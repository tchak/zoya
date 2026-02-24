import type { RouteInfo } from '../types';
import { groupByModule } from '../utils';
import { Card } from './Card';
import { ModuleHeader } from './ModuleHeader';

const METHOD_COLORS: Record<string, string> = {
  GET: 'bg-green-100 text-green-700',
  POST: 'bg-blue-100 text-blue-700',
  PUT: 'bg-yellow-100 text-yellow-700',
  PATCH: 'bg-orange-100 text-orange-700',
  DELETE: 'bg-red-100 text-red-700',
};

export function RoutesCard({ routes }: { routes: RouteInfo[] }) {
  return (
    <Card title="Routes">
      {routes.length === 0 ? (
        <p className="text-gray-400 text-sm italic">No routes</p>
      ) : (
        <div className="space-y-4">
          {groupByModule(routes, (a, b) =>
            a.pathname.localeCompare(b.pathname),
          ).map(([module, items]) => (
            <div key={module}>
              <ModuleHeader module={module} />
              <ul className="space-y-1.5">
                {items.map((r) => (
                  <li
                    key={`${r.method} ${r.pathname}`}
                    className="flex items-baseline gap-2"
                  >
                    <span
                      className={`inline-block px-1.5 py-0.5 rounded text-xs font-bold ${METHOD_COLORS[r.method] ?? 'bg-gray-100 text-gray-700'}`}
                    >
                      {r.method}
                    </span>
                    <code className="text-sm text-gray-900 font-mono">
                      {r.pathname}
                    </code>
                    <code className="text-xs text-gray-400 font-mono">
                      {r.handler}
                    </code>
                    <code className="text-xs text-gray-400 font-mono">
                      {r.signature}
                    </code>
                  </li>
                ))}
              </ul>
            </div>
          ))}
        </div>
      )}
    </Card>
  );
}
