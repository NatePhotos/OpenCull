// Exercise the actual development entrypoint, without creating an installer.
import { spawn } from 'node:child_process';
if (process.platform !== 'darwin') throw new Error('This smoke test requires macOS');
const child = spawn('npm', ['run', 'desktop', '--', '--no-watch'], { detached: true, stdio: ['ignore', 'pipe', 'pipe'] });
let log = '', exited = false;
child.on('exit', () => { exited = true; });
child.on('error', e => { log += e.message; exited = true; });
for (const stream of [child.stdout, child.stderr]) stream.on('data', chunk => {
  log = (log + chunk.toString()).slice(-120000);
  process.stdout.write(chunk);
});
const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));
try {
  const deadline = Date.now() + 12 * 60 * 1000;
  let started = false;
  while (Date.now() < deadline) {
    if (exited) throw new Error('Desktop development command exited before readiness');
    const plain = log.replace(/\x1b\[[0-9;]*m/g, '');
    if (/Running\s+`[^`]*target\/debug\/opencull`/.test(plain)) {
      const response = await fetch('http://localhost:1420');
      if (response.ok && (await response.text()).includes('/src/main.tsx')) { started = true; break; }
    }
    await sleep(1000);
  }
  if (!started) throw new Error('Timed out waiting for native desktop process and Vite frontend');
  await sleep(8000);
  if (exited) throw new Error('Desktop development command exited during startup');
  // Cargo's parent staying alive is insufficient if the app crashed. Check its child.
  const { execFileSync } = await import('node:child_process');
  const processes = execFileSync('ps', ['-axo', 'command='], { encoding: 'utf8' });
  if (!processes.split('\n').some(line => /target\/debug\/opencull\s*$/.test(line.trim()))) throw new Error('Native application process is not running');
  console.log('macOS dev desktop smoke passed: native app and Vite frontend remain running.');
} finally {
  try { process.kill(-child.pid, 'SIGTERM'); } catch { /* Already exited. */ }
}
