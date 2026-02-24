import { defineConfig } from 'eslint/config';
import js from '@eslint/js';
import tseslint from 'typescript-eslint';
import prettier from 'eslint-plugin-prettier/recommended';

export default defineConfig([
  { ignores: ['dist/'] },
  js.configs.recommended,
  tseslint.configs.recommended,
  prettier,
]);
