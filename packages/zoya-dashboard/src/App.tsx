import { useEffect, useState } from 'react';
import type { DashboardData } from './types';
import { Dashboard } from './components/Dashboard';

export function App() {
  const [data, setData] = useState<DashboardData | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    fetch('./api/data')
      .then((res) => {
        if (!res.ok) throw new Error(`HTTP ${res.status}`);
        return res.json();
      })
      .then(setData)
      .catch((err) => setError(err.message));
  }, []);

  if (error) {
    return (
      <div className="p-8 max-w-5xl mx-auto">
        <p className="text-red-600">Failed to load dashboard: {error}</p>
      </div>
    );
  }

  if (!data) {
    return (
      <div className="p-8 max-w-5xl mx-auto">
        <p className="text-gray-400">Loading...</p>
      </div>
    );
  }

  return <Dashboard data={data} />;
}
