import type { TaskInfo } from '../types';
import { groupByModule } from '../utils';
import { Card } from './Card';
import { ModuleHeader } from './ModuleHeader';

export function TasksCard({ tasks }: { tasks: TaskInfo[] }) {
  return (
    <Card title="Tasks">
      {tasks.length === 0 ? (
        <p className="text-gray-400 text-sm italic">No tasks</p>
      ) : (
        <div className="space-y-4">
          {groupByModule(tasks, (a, b) => a.name.localeCompare(b.name)).map(
            ([module, items]) => (
              <div key={module}>
                <ModuleHeader module={module} />
                <ul className="space-y-1.5">
                  {items.map((t) => (
                    <li key={t.name} className="flex items-baseline">
                      <code className="text-sm text-amber-600 font-mono">
                        {t.name}
                      </code>
                      <code className="text-xs text-gray-400 font-mono ml-2">
                        {t.signature}
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
