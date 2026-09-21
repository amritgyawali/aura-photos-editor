import react from '@vitejs/plugin-react';
import { defineConfig } from 'vite';

// Tauri serves the built assets from ../dist and needs a fixed dev port.
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 5173,
    strictPort: true,
    watch: {
      // The desktop shell's build artifacts live in src-tauri/target on some
      // machines and rustc's incremental dirs go stale under the watcher mid
      // scan, which crashes it with an UNKNOWN scandir. Nothing there is ever
      // an HMR input, so it is ignored unconditionally.
      ignored: ['**/src-tauri/**'],
    },
  },
  build: { outDir: 'dist', target: 'es2022', sourcemap: true },
  test: {
    environment: 'jsdom',
    globals: true,
    include: ['src/**/*.test.ts', 'src/**/*.test.tsx'],
  },
});
