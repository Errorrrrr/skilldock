import { readFileSync, writeFileSync, readdirSync } from 'node:fs'
import { join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { spawnSync } from 'node:child_process'

const root = resolve(fileURLToPath(new URL('..', import.meta.url)))
const read = (path) => readFileSync(join(root, path), 'utf8')
const stable = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/
const platforms = ['darwin-aarch64', 'darwin-x86_64', 'windows-x86_64', 'linux-x86_64']
export function validateVersions(tag, pkg, tauri, cargo, lock) {
  const version = tag.replace(/^v/, '')
  if (tag !== `v${version}` || !stable.test(version))
    throw new Error('发布标签必须为 v主版本.次版本.补丁版本（正式版本）')
  const workspace = cargo.match(/\[workspace\.package\]([\s\S]*?)(?=\n\[|$)/)?.[1]
  if (
    [pkg.version, tauri.version, workspace?.match(/version\s*=\s*"([^"]+)"/)?.[1]].some(
      (v) => v !== version,
    )
  )
    throw new Error('标签与 package.json、tauri.conf.json、Cargo.toml 版本不一致')
  for (const name of ['skilldock-core', 'skilldock-cli', 'skilldock-desktop']) {
    const entry = lock.split('[[package]]').find((block) => block.includes(`name = "${name}"`))
    if (entry?.match(/version = "([^"]+)"/)?.[1] !== version)
      throw new Error(`Cargo.lock 中 ${name} 的版本未同步`)
  }
  return version
}
export function validatePublicKey(value) {
  const text = Buffer.from(value.trim(), 'base64').toString('utf8')
  const lines = text.trim().split(/\r?\n/)
  const key = Buffer.from(lines.at(-1) || '', 'base64')
  if (
    !text.startsWith('untrusted comment:') ||
    key.length !== 42 ||
    key.subarray(0, 2).toString() !== 'Ed'
  )
    throw new Error('SKILLDOCK_UPDATER_PUBLIC_KEY 必须是 Tauri signer 生成的 .pub 文件内容')
  return value.trim()
}
export function validateManifest(manifest, version, assets, repo) {
  if (manifest.version?.replace(/^v/, '') !== version) throw new Error('更新清单版本不匹配')
  const prefix = `https://github.com/${repo}/releases/download/v${version}/`
  for (const platform of platforms) {
    if (!manifest.platforms?.[platform]) throw new Error(`更新清单缺少平台：${platform}`)
  }
  for (const [platform, item] of Object.entries(manifest.platforms || {})) {
    if (!item?.url?.startsWith(prefix) || !item.signature?.trim())
      throw new Error(`更新清单缺少有效平台：${platform}`)
    const name = decodeURIComponent(item.url.slice(prefix.length))
    if (
      name.includes('/') ||
      !assets.some((a) => a.name === name && a.size > 0) ||
      !assets.some((a) => a.name === `${name}.sig` && a.size > 0)
    )
      throw new Error(`Release 缺少更新包或签名：${platform}`)
  }
}
export function finalizeManifest(manifest, release, version, repo) {
  if (release.tag_name !== `v${version}` || !release.draft)
    throw new Error('只能准备当前版本的草稿 Release')
  const assets = release.assets || []
  for (const item of Object.values(manifest.platforms || {})) {
    const asset = assets.find((a) => a.url === item.url || a.browser_download_url === item.url)
    if (!asset) throw new Error('更新清单引用了不属于当前 Release 的附件')
    // Draft assets use an untagged-* URL; publication changes it to the version tag.
    item.url = `https://github.com/${repo}/releases/download/v${version}/${encodeURIComponent(asset.name)}`
  }
  manifest.notes = release.body || ''
  manifest.pub_date = new Date().toISOString()
  validateManifest(manifest, version, assets, repo)
  return manifest
}
function findSignatures(dir) {
  return readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
    const path = join(dir, entry.name)
    return entry.isDirectory() ? findSignatures(path) : entry.name.endsWith('.sig') ? [path] : []
  })
}
function main() {
  const [command, arg] = process.argv.slice(2)
  const config = JSON.parse(read('src-tauri/tauri.conf.json'))
  const version = validateVersions(
    process.env.GITHUB_REF_NAME || `v${config.version}`,
    JSON.parse(read('package.json')),
    config,
    read('Cargo.toml'),
    read('Cargo.lock'),
  )
  if (command === 'check') {
    console.log(`版本校验通过：${version}`)
    return
  }
  if (command === 'prepare') {
    const key = validatePublicKey(process.env.SKILLDOCK_UPDATER_PUBLIC_KEY || '')
    if (!process.env.TAURI_SIGNING_PRIVATE_KEY?.trim())
      throw new Error('缺少 TAURI_SIGNING_PRIVATE_KEY Secret')
    config.bundle.createUpdaterArtifacts = true
    config.plugins.updater.pubkey = key
    // Only the public key is written; the private key remains in the CI environment.
    writeFileSync(join(root, 'src-tauri/tauri.conf.json'), JSON.stringify(config, null, 2) + '\n')
    console.log('已准备签名更新构建配置')
    return
  }
  if (command === 'verify-local') {
    const signatures = findSignatures(join(root, 'target', arg, 'release', 'bundle'))
    if (!signatures.length) throw new Error('构建未生成更新签名')
    const result = spawnSync(
      'cargo',
      [
        'run',
        '--locked',
        '-p',
        'skilldock-desktop',
        '--example',
        'verify_update',
        '--',
        join(root, 'src-tauri/tauri.conf.json'),
        ...signatures,
      ],
      { stdio: 'inherit', cwd: root },
    )
    if (result.error || result.status !== 0) throw new Error('更新包签名与应用内置公钥不匹配')
    return
  }
  if (command === 'finalize-manifest') {
    const release = JSON.parse(readFileSync(process.env.RELEASE_INFO_FILE, 'utf8'))
    const manifest = finalizeManifest(
      JSON.parse(readFileSync(arg, 'utf8')),
      release,
      version,
      process.env.GITHUB_REPOSITORY,
    )
    writeFileSync(arg, JSON.stringify(manifest, null, 2) + '\n')
    console.log('更新说明、公开下载地址和全部平台附件已校验')
    return
  }
  throw new Error(
    '用法：node scripts/release.mjs check|prepare|verify-local TARGET|finalize-manifest FILE',
  )
}
if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    main()
  } catch (error) {
    console.error(error.message)
    process.exitCode = 1
  }
}
