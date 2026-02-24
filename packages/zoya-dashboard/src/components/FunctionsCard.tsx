import type { FunctionInfo } from '../types';
import { groupByModule } from '../utils';
import { Card } from './Card';
import { ModuleHeader } from './ModuleHeader';

export function FunctionsCard({ functions }: { functions: FunctionInfo[] }) {
  return (
    <Card title="Functions">
      {functions.length === 0 ? (
        <p className="text-gray-400 text-sm italic">No functions</p>
      ) : (
        <div className="space-y-4">
          {groupByModule(functions, (a, b) => a.name.localeCompare(b.name)).map(
            ([module, items]) => (
              <div key={module}>
                <ModuleHeader module={module} />
                <ul className="space-y-1.5">
                  {items.map((f) => (
                    <li key={f.name} className="flex items-baseline">
                      <code className="text-sm text-indigo-600 font-mono">
                        {f.name}
                      </code>
                      <code className="text-xs text-gray-400 font-mono ml-2">
                        {f.signature}
                      </code>
                    </li>
                  ))}
                </ul>
              </div>
            ),
          )}
        </div>
      )}
    </Card>
  );
}
