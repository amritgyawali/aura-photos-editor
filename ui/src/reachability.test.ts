import { describe, expect, it } from 'vitest';

/**
 * Every source file has to be reachable from `main.tsx`.
 *
 * ## Why this test exists
 *
 * The independent review of phases 01 to 30 found that **forty-two of the eighty-two non-test UI
 * source files were reachable from `main.tsx` by no import path at all** - the whole develop
 * stack, people, story, style, cull, cleanup, camera matching and most of the explain rail. Every
 * one of them had passing tests and every command behind them answered; nothing anywhere checked
 * that a photographer could get to them. `PHASE-01-30-REVIEW.md` section 6.4.
 *
 * A component test proves a component works. It cannot prove the component is *mounted*, and the
 * failure mode is silent in exactly the way the review described: the build is green, the tests
 * are green, the feature is finished, and the application does not have it.
 *
 * ## How
 *
 * `import.meta.glob` with `?raw` gives the bundler's own view of the source tree, so this needs
 * no filesystem access and runs the same way in CI as it does here. The walk is over static
 * `from '...'` specifiers, which is the whole graph: there are no dynamic imports in this front
 * end. If one is ever added, this starts reporting a file as unreachable that is in fact loaded,
 * and the fix is to teach the walker about it rather than to delete the test.
 *
 * ## What it does not prove
 *
 * That a panel is *usable* - behind a control somebody can find, sized properly, reachable
 * without a project open. It proves the weaker thing, which is that a static import path exists
 * from the entry point. That is the property whose absence went unnoticed for nineteen phases.
 */

const SOURCES = import.meta.glob('./**/*.{ts,tsx}', {
  query: '?raw',
  eager: true,
  import: 'default',
}) as Record<string, string>;

/** Extensions tried, in order, when resolving a relative specifier. */
const CANDIDATES = ['.tsx', '.ts', '/index.tsx', '/index.ts', ''];

/** `./a/../b/c` to `./b/c`, so two spellings of one file are one key. */
function normalise(path: string): string {
  const parts: string[] = [];
  for (const segment of path.split('/')) {
    if (segment === '.' || segment === '') {
      continue;
    }
    if (segment === '..') {
      parts.pop();
    } else {
      parts.push(segment);
    }
  }
  return `./${parts.join('/')}`;
}

function resolveSpecifier(importer: string, specifier: string): string | null {
  if (!specifier.startsWith('.')) {
    return null;
  }
  const directory = importer.slice(0, importer.lastIndexOf('/'));
  const base = normalise(`${directory}/${specifier}`);
  for (const suffix of CANDIDATES) {
    if (Object.prototype.hasOwnProperty.call(SOURCES, base + suffix)) {
      return base + suffix;
    }
  }
  return null;
}

function reachableFromEntry(): Set<string> {
  const seen = new Set<string>();
  const stack = ['./main.tsx'];
  while (stack.length > 0) {
    const current = stack.pop();
    if (current === undefined || seen.has(current)) {
      continue;
    }
    const source = SOURCES[current];
    if (source === undefined) {
      continue;
    }
    seen.add(current);
    for (const match of source.matchAll(/from\s+['"]([^'"]+)['"]/g)) {
      const next = resolveSpecifier(current, match[1] ?? '');
      if (next !== null) {
        stack.push(next);
      }
    }
  }
  return seen;
}

const SHIPPED = Object.keys(SOURCES).filter((path) => !path.includes('.test.'));

describe('the application can reach everything it ships', () => {
  it('sees the source tree at all', () => {
    // A glob that matched nothing would make every assertion below pass for the wrong reason.
    // Phase 21's rule about a refusal test that cannot tell a working guard from a broken
    // fixture, applied to a test whose fixture is the repository.
    expect(SHIPPED.length).toBeGreaterThan(50);
    expect(SOURCES['./main.tsx']).toBeDefined();
  });

  it('has no orphaned source file', () => {
    const reachable = reachableFromEntry();
    const orphans = SHIPPED.filter((path) => !reachable.has(path)).sort();
    expect(orphans).toEqual([]);
  });

  it('reaches every component directory', () => {
    // A weaker second assertion, and a deliberately different one: the first fails on one new
    // orphaned file, and this fails on a whole feature that was built and never wired up, which
    // is the shape the review actually found.
    const reachable = [...reachableFromEntry()];
    const directories = new Set(
      SHIPPED.map((path) => /^\.\/components\/([^/]+)\//.exec(path)?.[1]).filter(
        (name): name is string => name !== undefined,
      ),
    );
    expect(directories.size).toBeGreaterThan(10);
    for (const directory of directories) {
      expect(
        reachable.some((path) => path.startsWith(`./components/${directory}/`)),
        `nothing in components/${directory} is reachable from main.tsx`,
      ).toBe(true);
    }
  });
});
