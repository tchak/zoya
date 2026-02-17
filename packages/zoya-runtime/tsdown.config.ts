import { defineConfig } from 'tsdown';

export default defineConfig({
  entry: ['src/index.ts'],
  format: 'iife',
  outDir: 'dist',
  clean: true,
});
