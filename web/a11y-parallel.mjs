// Runs the console accessibility gate for the light and the dark theme at the same time (two processes, two ports), which about
// halves the wall-clock time of the 80-odd page check. Exits non-zero if either fails. Run: npx vite build && node a11y-parallel.mjs
import { spawn } from 'node:child_process'

const run = (theme, port) =>
  new Promise((resolve) => {
    const child = spawn(process.execPath, ['a11y-check.mjs', `--theme=${theme}`, `--port=${port}`, ...process.argv.slice(2)], {
      stdio: ['ignore', 'pipe', 'pipe'],
    })
    let out = ''
    child.stdout.on('data', (d) => (out += d))
    child.stderr.on('data', (d) => (out += d))
    child.on('close', (code) => resolve({ theme, code: code ?? 1, out }))
  })

const results = await Promise.all([run('light', 3111), run('dark', 3114)])
for (const r of results) {
  console.log(`=== ${r.theme} theme: ${r.code === 0 ? 'ok' : 'FAILED'}`)
  console.log(r.out.trim())
}
process.exit(results.some((r) => r.code !== 0) ? 1 : 0)
