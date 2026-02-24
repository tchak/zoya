import type { DashboardData } from '../types';
import { Badge } from './Badge';
import { FunctionsCard } from './FunctionsCard';
import { TestsCard } from './TestsCard';
import { TasksCard } from './TasksCard';
import { RoutesCard } from './RoutesCard';

export function Dashboard({ data }: { data: DashboardData }) {
  return (
    <div className="p-8 max-w-5xl mx-auto">
      <header className="mb-8">
        <h1 className="text-3xl font-bold text-gray-900">
          {data.package_name}
        </h1>
        <p className="text-gray-500 mt-1">Package Dashboard</p>
      </header>

      <div className="flex gap-3 mb-8">
        <Badge label="Functions" count={data.functions.length} />
        <Badge label="Tests" count={data.tests.length} />
        <Badge label="Tasks" count={data.tasks.length} />
        <Badge label="Routes" count={data.routes.length} />
      </div>

      <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
        <FunctionsCard functions={data.functions} />
        <TestsCard tests={data.tests} />
        <TasksCard tasks={data.tasks} />
        <RoutesCard routes={data.routes} />
      </div>
    </div>
  );
}
