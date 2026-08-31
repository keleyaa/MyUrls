import eslint from '@eslint/js';
import svelte from 'eslint-plugin-svelte';
import tseslint from 'typescript-eslint';

export default tseslint.config(
  {
    ignores: [
      '**/dist/**',
      '**/node_modules/**',
      'test-results/**',
      'playwright-report/**',
      'output/**',
      'legacy/**',
    ],
  },
  eslint.configs.recommended,
  ...tseslint.configs.recommended,
  ...svelte.configs['flat/recommended'],
  {
    files: ['**/*.svelte'],
    languageOptions: {
      parserOptions: {
        extraFileExtensions: ['.svelte'],
        parser: tseslint.parser,
      },
    },
    rules: {
      'no-undef': 'off',
      'no-unassigned-vars': 'off',
    },
  },
  {
    files: ['**/*.{ts,tsx,mts,cts}'],
    rules: {
      '@typescript-eslint/consistent-type-imports': ['error', { prefer: 'type-imports' }],
      '@typescript-eslint/no-unused-vars': [
        'error',
        { argsIgnorePattern: '^_', caughtErrorsIgnorePattern: '^_' },
      ],
    },
  },
  {
    files: ['ops/**/*.mjs'],
    languageOptions: {
      globals: {
        AbortSignal: 'readonly',
        clearTimeout: 'readonly',
        fetch: 'readonly',
        process: 'readonly',
        setTimeout: 'readonly',
      },
    },
  },
  {
    files: ['**/*.svelte'],
    rules: {
      'svelte/valid-compile': 'error',
    },
  },
);
