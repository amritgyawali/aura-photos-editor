import react from '@vitejs/plugin-react';
import { defineConfig } from 'vite';

// Tauri serves the built assets from ../dist and needs a fixed dev port.
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  // The Rust build output under src-tauri is not UI source; watching it crashes the dev server
  // when cargo rewrites (or the disk garbles) an incremental directory mid-scan.
  server: { port: 5173, strictPort: true, watch: { ignored: ['**/src-tauri/**', '**/dist/**'] } },
  build: { outDir: 'dist', target: 'es2022', sourcemap: true },
  test: {
    environment: 'jsdom',
    globals: true,
    include: ['src/**/*.test.ts', 'src/**/*.test.tsx'],
  },
});
