// Local preview: `node dev.mjs`, then http://localhost:3000. Serves public/ and the api/ handlers
// the way Vercel does, so the pages and the licence server can be tried before deploying.
import { createServer } from 'node:http';
import { readFile } from 'node:fs/promises';
import { extname, join } from 'node:path';

const types = { '.html': 'text/html', '.css': 'text/css', '.js': 'text/javascript', '.txt': 'text/plain; charset=utf-8' };
createServer(async (req, res) => {
  const url = new URL(req.url, 'http://localhost');
  if (url.pathname.startsWith('/api/')) {
    const { default: handler } = await import(`./api/${url.pathname.slice(5)}.js`);
    const shim = Object.assign(res, {
      status(code) { res.statusCode = code; return shim; },
      json(body) { res.setHeader('Content-Type', 'application/json'); res.end(JSON.stringify(body)); },
    });
    req.query = Object.fromEntries(url.searchParams);
    return handler(req, shim);
  }
  const path = url.pathname === '/' ? '/index.html' : extname(url.pathname) ? url.pathname : `${url.pathname}.html`;
  try {
    const body = await readFile(join('public', path));
    res.setHeader('Content-Type', types[extname(path)] || 'application/octet-stream');
    res.end(body);
  } catch { res.statusCode = 404; res.end('Not found'); }
}).listen(process.env.PORT || 3000, () => console.log(`http://localhost:${process.env.PORT || 3000}`));
