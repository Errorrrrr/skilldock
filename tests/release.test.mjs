import { test } from 'node:test'
import assert from 'node:assert/strict'
import {
  validateVersions,
  validatePublicKey,
  validateManifest,
  finalizeManifest,
} from '../scripts/release.mjs'
const cargo = '[workspace.package]\nversion = "1.2.3"\n[workspace.dependencies]'
const lock = ['skilldock-core', 'skilldock-cli', 'skilldock-desktop']
  .map((name) => `[[package]]\nname = "${name}"\nversion = "1.2.3"`)
  .join('\n')
test('release requires matching stable tag and all application versions', () => {
  assert.equal(
    validateVersions('v1.2.3', { version: '1.2.3' }, { version: '1.2.3' }, cargo, lock),
    '1.2.3',
  )
  for (const tag of ['main', '1.2.3', 'v1.2.4', 'v1.2.3-beta.1', 'v01.2.3'])
    assert.throws(() =>
      validateVersions(tag, { version: '1.2.3' }, { version: '1.2.3' }, cargo, lock),
    )
  assert.throws(() =>
    validateVersions(
      'v1.2.3',
      { version: '1.2.3' },
      { version: '1.2.3' },
      cargo,
      lock.replace('1.2.3', '1.0.0'),
    ),
  )
})
test('public key requires Tauri encoded minisign format', () => {
  for (const key of ['', 'placeholder', '-----BEGIN PRIVATE KEY-----'])
    assert.throws(() => validatePublicKey(key))
  const data = Buffer.alloc(42)
  data.write('Ed')
  const key = Buffer.from(
    `untrusted comment: test public key\n${data.toString('base64')}\n`,
  ).toString('base64')
  assert.equal(validatePublicKey(key), key)
})
test('manifest requires all signed platform artifacts from this exact release', () => {
  const platforms = ['darwin-aarch64', 'darwin-x86_64', 'windows-x86_64', 'linux-x86_64']
  const manifest = {
    version: '1.2.3',
    platforms: Object.fromEntries(
      platforms.map((p) => [
        p,
        {
          url: `https://github.com/Errorrrrr/skilldock/releases/download/v1.2.3/${p}.zip`,
          signature: 'signature',
        },
      ]),
    ),
  }
  const assets = platforms.flatMap((p) => [
    { name: `${p}.zip`, size: 100 },
    { name: `${p}.zip.sig`, size: 100 },
  ])
  validateManifest(manifest, '1.2.3', assets, 'Errorrrrr/skilldock')
  assert.throws(() => validateManifest(manifest, '1.2.3', assets.slice(1), 'Errorrrrr/skilldock'))
  assert.throws(() => validateManifest(manifest, '1.2.4', assets, 'Errorrrrr/skilldock'))
  assert.throws(() => validateManifest(manifest, '1.2.3', assets, 'someone/else'))
  const missing = structuredClone(manifest)
  delete missing.platforms['darwin-aarch64']
  assert.throws(() => validateManifest(missing, '1.2.3', assets, 'Errorrrrr/skilldock'))
})

test('finalization maps v1 action API asset URLs to public URLs and copies release notes', () => {
  const platforms = ['darwin-aarch64', 'darwin-x86_64', 'windows-x86_64', 'linux-x86_64']
  const assets = platforms.flatMap((p, i) => [
    {
      name: `${p}.zip`,
      size: 100,
      url: `https://api.github.com/repos/Errorrrrr/skilldock/releases/assets/${i}`,
      browser_download_url: `https://github.com/Errorrrrr/skilldock/releases/download/untagged-draft123/${p}.zip`,
    },
    { name: `${p}.zip.sig`, size: 100 },
  ])
  const manifest = {
    version: '1.2.3',
    platforms: Object.fromEntries(
      platforms.map((p, i) => [p, { url: assets[i * 2].url, signature: 'signature' }]),
    ),
  }
  const release = { tag_name: 'v1.2.3', draft: true, assets, body: '发布说明' }
  const result = finalizeManifest(
    structuredClone(manifest),
    release,
    '1.2.3',
    'Errorrrrr/skilldock',
  )
  assert.equal(result.notes, '发布说明')
  assert.equal(
    result.platforms['darwin-aarch64'].url,
    'https://github.com/Errorrrrr/skilldock/releases/download/v1.2.3/darwin-aarch64.zip',
  )
  const draftUrls = structuredClone(manifest)
  platforms.forEach((p, i) => {
    draftUrls.platforms[p].url = assets[i * 2].browser_download_url
  })
  assert.deepEqual(
    finalizeManifest(draftUrls, release, '1.2.3', 'Errorrrrr/skilldock').platforms,
    result.platforms,
  )
  assert.throws(() =>
    finalizeManifest(
      structuredClone(manifest),
      { ...release, draft: false },
      '1.2.3',
      'Errorrrrr/skilldock',
    ),
  )
  assert.throws(() =>
    finalizeManifest(
      structuredClone(manifest),
      { ...release, assets: assets.slice(1) },
      '1.2.3',
      'Errorrrrr/skilldock',
    ),
  )
})
