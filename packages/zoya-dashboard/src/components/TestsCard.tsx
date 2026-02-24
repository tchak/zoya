import type { TestInfo } from '../types';
import { groupByModule } from '../utils';
import { Card } from './Card';
import { ModuleHeader } from './ModuleHeader';

export function TestsCard({ tests }: { tests: TestInfo[] }) {
  return (
    <Card title="Tests">
      {tests.length === 0 ? (
        <p className="text-gray-400 text-sm italic">No tests</p>
      ) : (
        <div className="space-y-4">
          {groupByModule(tests, (a, b) => a.name.localeCompare(b.name)).map(
            ([module, items]) => (
              <div key={module}>
                <ModuleHeader module={module} />
                <ul className="space-y-1.5">
                  {items.map((t) => (
                    <li key={t.name} className="flex items-baseline">
                      <code className="text-sm text-gray-700 font-mono">
                        {t.name}
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
