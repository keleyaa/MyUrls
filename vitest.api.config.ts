import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    include: ['apps/server/src/**/*.api.test.ts'],
    environment: 'node',
  },
});
