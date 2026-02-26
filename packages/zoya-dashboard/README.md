# zoya-dashboard

Developer dashboard SPA for the Zoya programming language.

A React application that displays package metadata (functions, tests, jobs, HTTP routes) during development. Served by the Rust `zoya-dashboard` crate as an embedded SPA when running `zoya dev`.

## Features

- **Package overview** - Displays all public functions, tests, jobs, and HTTP routes
- **Module grouping** - Items organized by module with collapsible sections
- **HTTP route display** - Method badges (GET/POST/PUT/PATCH/DELETE) with pathnames and signatures
- **Function signatures** - Type signatures displayed for all functions and handlers

## Components

| Component       | Description                                     |
| --------------- | ----------------------------------------------- |
| `App`           | Root component; fetches data from `/api/data`   |
| `Dashboard`     | Main layout with badge summary and card grid    |
| `FunctionsCard` | All functions grouped by module with signatures |
| `TestsCard`     | Test functions grouped by module                |
| `JobsCard`      | Job functions with signatures                   |
| `RoutesCard`    | HTTP routes with method badges and pathnames    |
| `ModuleHeader`  | Module name section header                      |
| `Badge`         | Count badge (Functions, Tests, Jobs, Routes)    |
| `Card`          | Reusable card container                         |

## Stack

- **React 19** + TypeScript
- **Vite** - Build tool and dev server
- **Tailwind CSS v4** - Utility-first styling

## Development

```bash
npm run dev         # Start Vite dev server
npm run build       # Build for production
npm run preview     # Preview production build
npm test            # Run tests with vitest
npm run typecheck   # Type-check with tsc
npm run lint        # Lint with eslint
npm run format      # Format with prettier
```

## Integration

The built `dist/` directory is committed to the repository. The Rust `zoya-dashboard` crate embeds these files via `include_str!()` and serves them as an Axum router. The SPA fetches package data from `./api/data` (relative to its `<base href>`), which allows it to be mounted at any path prefix (e.g., `/_`).
