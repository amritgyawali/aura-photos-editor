import react from '@vitejs/plugin-react';
import { defineConfig } from 'vite';

// Tauri serves the built assets from ../dist and needs a fixed dev port.
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: { port: 5173, strictPort: true },
  build: { outDir: 'dist', target: 'es2022', sourcemap: true },
  test: {
    environment: 'jsdom',
    globals: true,
    include: ['src/**/*.test.ts', 'src/**/*.test.tsx'],
  },
});
