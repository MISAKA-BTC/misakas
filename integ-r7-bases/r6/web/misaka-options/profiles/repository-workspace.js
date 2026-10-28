/* Non-authoritative working copies on the publisher's device. No network writes,
 * private keys or VPS storage. A draft is NOT a signed repository manifest. */
export const workspaceOwner = (account) => account ? `wallet:${String(account).toLowerCase()}` : 'device:local';
export const ownerLabel = (account) => account ? `${account.slice(0, 8)}…${account.slice(-6)}` : 'This device (local)';
export const IGNORE_TEMPLATES = {
  None: '', Node: 'node_modules/\ndist/\n.env\n.env.*\n',
  Python: '__pycache__/\n*.py[cod]\n.venv/\n.env\n', Rust: 'target/\n.env\n',
};
const MIT = (year) => `MIT License\n\nCopyright (c) ${year} [copyright holder]\n\nPermission is hereby granted, free of charge, to any person obtaining a copy\nof this software and associated documentation files (the "Software"), to deal\nin the Software without restriction, including without limitation the rights\nto use, copy, modify, merge, publish, distribute, sublicense, and/or sell\ncopies of the Software, and to permit persons to whom the Software is\nfurnished to do so, subject to the following conditions:\n\nThe above copyright notice and this permission notice shall be included in all\ncopies or substantial portions of the Software.\n\nTHE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR\nIMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,\nFITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE\nAUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER\nLIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,\nOUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE\nSOFTWARE.\n`;
export function validateRepository(input) {
  const name = String(input.name || '').trim(), description = String(input.description || '').trim();
  if (!/^[A-Za-z0-9][A-Za-z0-9._-]{0,99}$/.test(name) || name.toLowerCase().endsWith('.git'))
    throw new Error('Use 1–100 letters, numbers, dots, hyphens or underscores, starting with a letter or number. Do not use a .git suffix.');
  if (Array.from(description).length > 350) throw new Error('Description must be 350 characters or fewer.');
  if (input.visibility !== 'public') throw new Error('Private publishing requires encryption and is not supported yet.');
  if (!Object.hasOwn(IGNORE_TEMPLATES, input.gitignore)) throw new Error('Choose a supported .gitignore template.');
  if (!['None', 'MIT'].includes(input.license)) throw new Error('Choose a supported license template.');
  return { name, description, visibility: 'public', readme: !!input.readme, gitignore: input.gitignore, license: input.license };
}
export async function makeRepository(input, owner, now = new Date().toISOString()) {
  const data = validateRepository(input);
  if (owner !== 'device:local' && !/^wallet:0x[0-9a-f]{40}$/.test(owner)) throw new Error('Invalid local workspace owner.');
  const files = [];
  if (data.readme) files.push({ path: 'README.md', content: `# ${data.name}\n\n${data.description}\n` });
  if (IGNORE_TEMPLATES[data.gitignore]) files.push({ path: '.gitignore', content: IGNORE_TEMPLATES[data.gitignore] });
  if (data.license === 'MIT') files.push({ path: 'LICENSE', content: MIT(new Date(now).getUTCFullYear()) });
  for (const file of files) {
    file.sha256 = Array.from(new Uint8Array(await crypto.subtle.digest('SHA-256', new TextEncoder().encode(file.content))), b => b.toString(16).padStart(2, '0')).join('');
  }
  return { schema: 'misaka/repository-draft/v1', key: `${owner}/${data.name.toLowerCase()}`, ownerScope: owner,
    intendedVisibility: data.visibility, name: data.name, description: data.description,
    defaultBranch: 'main', licenseTemplate: data.license, createdAt: now, updatedAt: now, files,
    publication: { signed: false, seeded: false, chainIdentityVerified: false } };
}
export function validStoredRepository(record, owner) {
  return record?.schema === 'misaka/repository-draft/v1' && record.ownerScope === owner
    && /^[A-Za-z0-9][A-Za-z0-9._-]{0,99}$/.test(record.name) && !record.name.toLowerCase().endsWith('.git')
    && record.key === `${owner}/${record.name.toLowerCase()}` && typeof record.description === 'string'
    && Array.from(record.description).length <= 350 && record.intendedVisibility === 'public'
    && typeof record.createdAt === 'string' && Number.isFinite(Date.parse(record.createdAt))
    && typeof record.updatedAt === 'string' && Number.isFinite(Date.parse(record.updatedAt))
    && record.defaultBranch === 'main' && ['None','MIT'].includes(record.licenseTemplate)
    && Array.isArray(record.files) && record.files.length <= 3
    && record.files.every(f => ['README.md', '.gitignore', 'LICENSE'].includes(f.path)
      && typeof f.content === 'string' && f.content.length < 20000 && /^[a-f0-9]{64}$/.test(f.sha256))
    && record.publication?.signed === false && record.publication?.seeded === false && record.publication?.chainIdentityVerified === false;
}
export class LocalRepositoryProvider {
  async db() {
    if (!globalThis.indexedDB) throw new Error('Browser storage is unavailable. Enable storage to create a local working copy.');
    if (!this.opening) this.opening = new Promise((resolve, reject) => {
      const request = indexedDB.open('misaka-publisher-workspace-v1', 1);
      request.onupgradeneeded = () => request.result.createObjectStore('repositories', { keyPath: 'key' });
      request.onsuccess = () => resolve(request.result);
      request.onerror = () => { this.opening = null; reject(new Error('Could not open browser storage.')); };
      request.onblocked = () => { this.opening = null; reject(new Error('Another tab is blocking workspace storage. Close it and retry.')); };
    });
    return this.opening;
  }
  async transact(mode, operation) {
    const db = await this.db();
    return new Promise((resolve, reject) => {
      const transaction = db.transaction('repositories', mode), request = operation(transaction.objectStore('repositories'));
      transaction.oncomplete = () => resolve(request.result);
      transaction.onabort = () => reject(new Error(transaction.error?.name === 'ConstraintError' ? 'A repository with this name already exists in this workspace.' : 'Could not save/read the working copy. Browser storage may be full or unavailable.'));
      transaction.onerror = () => {}; // transaction abort reports the error once
    });
  }
  async create(input, owner) {
    const record = await makeRepository(input, owner);
    await this.transact('readwrite', store => store.add(record));
    return record;
  }
  async list(owner) {
    return (await this.transact('readonly', store => store.getAll())).filter(record => validStoredRepository(record, owner));
  }
  async get(owner, name) {
    const record = await this.transact('readonly', store => store.get(`${owner}/${String(name).toLowerCase()}`));
    return validStoredRepository(record, owner) ? record : null;
  }
}
export const repositoryProvider = new LocalRepositoryProvider();
export function exportRepository(record) {
  if (!validStoredRepository(record, record.ownerScope)) throw new Error('Invalid working copy.');
  return JSON.stringify(record, null, 2) + '\n';
}
