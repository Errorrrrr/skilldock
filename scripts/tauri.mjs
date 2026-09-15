import { spawnSync } from 'node:child_process'
import { homedir } from 'node:os'
import { delimiter, join } from 'node:path'

// GUI terminals may not load Rust's shell setup. Preserve an existing Cargo first.
const cargo = process.platform === 'win32' ? 'cargo.exe' : 'cargo'
let check = spawnSync(cargo, ['--version'], { stdio: 'ignore' })
if (check.error?.code === 'ENOENT') {
  const pathKey = Object.keys(process.env).find((key) => key.toLowerCase() === 'path') || 'PATH'
  const cargoBin = join(process.env.CARGO_HOME || join(homedir(), '.cargo'), 'bin')
  process.env[pathKey] = [process.env[pathKey], cargoBin].filter(Boolean).join(delimiter)
  check = spawnSync(cargo, ['--version'], { stdio: 'ignore' })
}
if (check.error || check.status !== 0) {
  console.error('无法运行 Cargo。请安装 Rust 工具链，或检查 CARGO_HOME 与 PATH 配置。')
  process.exit(1)
}

// Unconfigured optional CI credentials must behave like absent environment variables.
for (const key of [
  'APPLE_CERTIFICATE',
  'APPLE_CERTIFICATE_PASSWORD',
  'APPLE_SIGNING_IDENTITY',
  'APPLE_ID',
  'APPLE_PASSWORD',
  'APPLE_TEAM_ID',
]) {
  if (process.env[key] === '') delete process.env[key]
}

// Run in this process so Ctrl+C retains the Tauri CLI's normal cleanup behavior.
await import('@tauri-apps/cli/tauri.js')
