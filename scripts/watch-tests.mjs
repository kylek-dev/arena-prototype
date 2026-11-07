import { spawn } from 'node:child_process';

// Keep the dependency-free mouse/input suite and TypeScript/DOM suite watching
// together without introducing a task-runner dependency.
const children = [
  spawn(process.execPath, ['--test', '--watch', 'tests/unit/*.test.mjs'], {
    stdio: 'inherit',
  }),
  spawn(process.execPath, ['node_modules/vitest/vitest.mjs', '--watch'], {
    stdio: 'inherit',
  }),
];

for (const signal of ['SIGINT', 'SIGTERM']) {
  process.on(signal, () => {
    for (const child of children) child.kill(signal);
  });
}
