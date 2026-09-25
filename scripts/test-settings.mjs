import { createRequire } from 'node:module'
const require = createRequire(import.meta.url)
const { build } = createRequire(require.resolve('vite/package.json'))('esbuild')
import { mkdtemp, rm } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { spawnSync } from 'node:child_process'
const directory = await mkdtemp(join(tmpdir(), 'skilldock-settings-test-'))
try {
  const outfile = join(directory, 'settings.test.mjs')
  await build({
    entryPoints: ['tests/settings-updates.test.ts'],
    bundle: true,
    platform: 'node',
    format: 'esm',
    outfile,
    logLevel: 'silent',
  })
  const result = spawnSync(process.execPath, ['--test', outfile], { stdio: 'inherit' })
  process.exitCode = result.status ?? 1
} finally {
  await rm(directory, { recursive: true, force: true })
}
