import type { JobInfo } from '../types';
import { groupByModule } from '../utils';
import { Card } from './Card';
import { ModuleHeader } from './ModuleHeader';

export function JobsCard({ jobs }: { jobs: JobInfo[] }) {
  return (
    <Card title="Jobs">
      {jobs.length === 0 ? (
        <p className="text-gray-400 text-sm italic">No jobs</p>
      ) : (
        <div className="space-y-4">
          {groupByModule(jobs, (a, b) => a.name.localeCompare(b.name)).map(
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
