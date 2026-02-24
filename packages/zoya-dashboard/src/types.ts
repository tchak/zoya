export interface DashboardData {
  package_name: string;
  functions: FunctionInfo[];
  tests: TestInfo[];
  jobs: JobInfo[];
  routes: RouteInfo[];
}

export interface FunctionInfo {
  name: string;
  module: string;
  signature: string;
}

export interface TestInfo {
  name: string;
  module: string;
}

export interface JobInfo {
  name: string;
  module: string;
  signature: string;
}

export interface RouteInfo {
  method: string;
  pathname: string;
  handler: string;
  module: string;
  signature: string;
}
