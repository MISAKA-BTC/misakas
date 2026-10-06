import { wrapAccountWorkspace } from './me.js?v=20261006-repositories3';
import { repositoryProvider, workspaceOwner, ownerLabel, exportRepository } from './repository-workspace.js?v=20261006-repositories3';
const escape = value => String(value ?? '').replace(/[&<>"']/g, c => ({ '&':'&amp;', '<':'&lt;', '>':'&gt;', '"':'&quot;', "'":'&#39;' })[c]);
const repoRoute = name => '#/me/repositories/' + encodeURIComponent(name);
const notice = '<p class="mr-notice">Local working copy · unsigned · not yet on MISAKA Torrent. Files stay on this device; no repository content is sent to the VPS. Public publication needs an owner-signed manifest and a running peer seeder. Clearing browser data removes this copy: export a backup.</p>';
function errorBox(main, message) { const box = main.querySelector('#mrError'); box.textContent = message; box.hidden = false; }
export async function renderRepositoryWorkspace({ main, account, path, alive = () => true }) {
  const owner = workspaceOwner(account);
  const shell = content => wrapAccountWorkspace({ account, active:'repositories', content:`<div class="mr-workspace">${content}</div>` });
  if (path === 'new') {
    main.innerHTML = shell(`<header class="mr-heading"><h1>Create a new repository</h1><p>Initialize a project's files on your device, ready to export for signed peer publication.</p><p>Required fields are marked with an asterisk (*).</p></header>${notice}
      <form id="mrCreate"><section class="mr-section"><h2><span class="mr-step">1</span> General</h2>
      <div class="mr-owner-name"><label>Owner *<input aria-label="Repository owner" readonly value="${escape(ownerLabel(account))}" required></label><span aria-hidden="true">/</span><label>Repository name *<input id="mrName" name="name" required maxlength="100" autocomplete="off" placeholder="my-model" pattern="[A-Za-z0-9][A-Za-z0-9._-]{0,99}"></label></div>
      <p class="mr-hint">Short, memorable names work best. This is a local workspace owner, not a verified chain username. The owner is fixed for this form; to use another wallet, connect it and reopen New.</p>
      <label class="mr-field">Description <span id="mrCount">0 / 350 characters</span><textarea id="mrDescription" name="description" rows="2" maxlength="700"></textarea></label>
      </section><section class="mr-section"><h2><span class="mr-step">2</span> Configuration</h2>
      <fieldset><legend>Choose visibility *</legend><label class="mr-option"><input type="radio" name="visibility" value="public" checked> <span><strong>Public — after publication</strong><small>The working copy is local only until it is signed and seeded.</small></span></label>
      <label class="mr-option"><input type="radio" name="visibility" value="private" disabled> <span><strong>Private (not available)</strong><small>Private Torrent publication needs encryption and access control.</small></span></label></fieldset>
      <label class="mr-option"><input type="checkbox" name="readme" checked><span><strong>Add README</strong><small>Use README.md as your longer project description.</small></span></label>
      <label class="mr-field">Add .gitignore<select name="gitignore"><option>None</option><option>Node</option><option>Python</option><option>Rust</option></select><small>Exclude generated files and environment files from future Git tracking.</small></label>
      <label class="mr-field">Add license<select name="license"><option>None</option><option>MIT</option></select><small>Review the license and fill in its copyright holder before publishing.</small></label>
      </section><p id="mrError" class="mr-error" role="alert" hidden></p><div class="mr-submit"><a href="#/me/repositories">Cancel</a><button class="mr-primary" type="submit">Create repository locally</button></div></form>`);
    const form = main.querySelector('#mrCreate'), count = main.querySelector('#mrCount');
    form.elements.description.addEventListener('input', () => {
      const length = Array.from(form.elements.description.value).length;
      count.textContent = `${length} / 350 characters`;
      form.elements.description.setCustomValidity(length > 350 ? 'Maximum 350 characters.' : '');
    });
    form.addEventListener('submit', async event => {
      event.preventDefault();
      const button = form.querySelector('button[type="submit"]'); button.disabled = true;
      main.querySelector('#mrError').hidden = true;
      try {
        const record = await repositoryProvider.create({ name:form.elements.name.value, description:form.elements.description.value,
          visibility:form.elements.visibility.value, readme:form.elements.readme.checked,
          gitignore:form.elements.gitignore.value, license:form.elements.license.value }, owner);
        if (alive()) location.hash = repoRoute(record.name);
      } catch (error) { if (alive()) errorBox(main, error.message); }
      finally { button.disabled = false; }
    });
    return;
  }
  main.innerHTML = shell('<p role="status">Reading repositories from this device…</p>');
  try {
    if (path.startsWith('repositories/')) {
      const name = decodeURIComponent(path.slice('repositories/'.length)), record = await repositoryProvider.get(owner, name);
      if (!alive()) return;
      if (!record) { main.innerHTML = shell('<h1>Repository not found on this device</h1><p>Check the connected wallet or return to your local repositories.</p><a href="#/me/repositories">Repositories</a>'); return; }
      main.innerHTML = shell(`<header class="mr-heading"><a href="#/me/repositories">← Repositories</a><h1>${escape(record.name)} <span class="mr-badge">Local</span></h1><p>${escape(record.description)}</p></header>${notice}
        <div class="mr-repo-tabs"><strong>Files</strong><span>main · initialized files, not Git history</span><button id="mrExport" type="button">Export working copy</button></div>
        <div class="mr-files">${record.files.length ? record.files.map(file => `<details><summary>${escape(file.path)} <span>${new TextEncoder().encode(file.content).length} bytes</span></summary><pre>${escape(file.content)}</pre><small>SHA-256: ${escape(file.sha256)}</small></details>`).join('') : '<p>No files initialized. Export includes the repository metadata.</p>'}</div>
        <section id="mrExportPanel" class="mr-section" hidden><h2>Export unsigned working copy</h2><label class="mr-field">Backup JSON<textarea id="mrExportText" readonly rows="8"></textarea></label><div class="mr-export-actions"><button type="button" id="mrDownload">Download JSON</button><button type="button" id="mrCopy">Copy JSON</button><span id="mrExportStatus" role="status"></span></div></section>
        <section class="mr-section"><h2>Publish from your PC</h2><p>This export is an unsigned draft, not an onchain registration or a public repository. Owner-signing and generic repository seeding are not connected to this form yet. Do not upload keys to a VPS.</p></section><p id="mrError" class="mr-error" role="alert" hidden></p>`);
      main.querySelector('#mrExport').addEventListener('click', () => {
        try {
          main.querySelector('#mrExportText').value = exportRepository(record);
          main.querySelector('#mrExportPanel').hidden = false;
          main.querySelector('#mrExportText').focus();
        } catch (error) { errorBox(main,error.message); }
      });
      main.querySelector('#mrDownload').addEventListener('click', () => {
        const url = URL.createObjectURL(new Blob([exportRepository(record)], { type:'application/json' }));
        const link = document.createElement('a'); link.href=url; link.download=record.name+'.misaka-draft.json';
        document.body.append(link); link.click(); link.remove(); setTimeout(() => URL.revokeObjectURL(url), 1000);
      });
      main.querySelector('#mrCopy').addEventListener('click', async () => {
        try { await navigator.clipboard.writeText(exportRepository(record)); main.querySelector('#mrExportStatus').textContent='Copied unsigned draft.'; }
        catch { main.querySelector('#mrExportText').select(); main.querySelector('#mrExportStatus').textContent='Select and copy the JSON above with your browser.'; }
      });
      return;
    }
    const records = await repositoryProvider.list(owner);
    if (!alive()) return;
    main.innerHTML = shell(`<div class="mr-toolbar"><input id="mrSearch" type="search" aria-label="Find a repository" placeholder="Find a repository…"><label>Type<select id="mrType"><option value="all">All local</option><option value="readme">With README</option></select></label><label>Sort<select id="mrSort"><option value="recent">Last created</option><option value="name">Name</option></select></label><a class="mr-primary" href="#/me/new">▣ New</a></div>${notice}<p id="mrResults" role="status"></p><div id="mrRepositories"></div>`);
    const update = () => {
      const query=main.querySelector('#mrSearch').value.toLowerCase(), type=main.querySelector('#mrType').value, sort=main.querySelector('#mrSort').value;
      const filtered=records.filter(record => (record.name+' '+record.description).toLowerCase().includes(query) && (type !== 'readme' || record.files.some(f => f.path === 'README.md')));
      filtered.sort((a,b) => sort === 'name' ? a.name.localeCompare(b.name) : b.createdAt.localeCompare(a.createdAt));
      main.querySelector('#mrResults').textContent = `${filtered.length} local ${filtered.length === 1 ? 'repository' : 'repositories'}`;
      main.querySelector('#mrRepositories').innerHTML = filtered.length ? filtered.map(record => `<article class="mr-repository"><h2><a href="${repoRoute(record.name)}">${escape(record.name)}</a> <span class="mr-badge">Local · unsigned</span></h2><p>${escape(record.description)}</p><small>main · ${record.files.length} files · ${escape(record.createdAt.slice(0,10))}</small></article>`).join('') : '<div class="mp-self-empty"><strong>No local repositories found</strong><p>Create a working copy with New. Signed public repositories are separate from this device-only list.</p></div>';
    };
    for (const id of ['#mrSearch','#mrType','#mrSort']) main.querySelector(id).addEventListener(id === '#mrSearch' ? 'input' : 'change',update);
    update();
  } catch(error) { if (alive()) main.innerHTML=shell(`<p class="mr-error" role="alert">${escape(error.message)}</p><a href="#/me/new">New repository</a>`); }
}
