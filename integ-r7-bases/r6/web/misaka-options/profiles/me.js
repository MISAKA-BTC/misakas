/* Wallet-scoped profile. This is a view of the connected wallet, not a claim to a username. */
import { profileTabIcon } from './nav-icons.js';
import { avatarSvg } from './avatar.js?v=20261006-identicon1';
const escape = (value) => String(value ?? '').replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[c]);
const tabs = [
  ['overview', 'Overview'], ['repositories', 'Repositories'], ['models', 'Models'],
  ['stars', 'Stars'], ['followers', 'Followers'], ['following', 'Following'],
  ['memberships', 'Memberships'], ['developer', 'Developer'],
];
const route = (tab) => tab === 'memberships' ? '#/portfolio' : tab === 'developer' ? '#/dev' : '#/me' + (tab === 'overview' ? '' : '/' + tab);
const empty = (title, detail) => `<div class="mp-self-empty"><strong>${escape(title)}</strong><p>${escape(detail)}</p></div>`;
const panel = (title, content, extra = '') => `<section class="mp-self-panel"><div class="mp-self-panel-head"><h2>${escape(title)}</h2>${extra}</div>${content}</section>`;

export function wrapAccountWorkspace({ account, active = 'overview', content }) {
  const selected = tabs.some(([key]) => key === active) ? active : 'overview';
  const short = account ? `${account.slice(0, 8)}…${account.slice(-6)}` : 'No wallet connected';
  const nav = tabs.map(([key, label]) => `<a href="${route(key)}" class="mp-self-tab${key === selected ? ' selected' : ''}"${key === selected ? ' aria-current="page"' : ''}>${profileTabIcon(key)}<span>${label}</span></a>`).join('');
  return `<div class="mp-self">
    <div class="mp-self-heading"><span class="mp-self-mark" aria-hidden="true">M</span><span>${account ? 'My profile' : 'Profile'}</span></div>
    <nav class="mp-self-tabs" aria-label="Profile sections">${nav}</nav>
    <div class="mp-self-layout"><aside class="mp-self-sidebar">
      <div class="mp-self-avatar" aria-hidden="true">${avatarSvg(account || 'misaka:local-device', {namespace:account?'evm':'local'})}</div>
      <h1>${account ? 'MISAKA user' : 'Your profile'}</h1><p class="mp-self-handle">${escape(short)}</p>
      <p class="mp-self-bio">${account ? 'Wallet profile · username not claimed' : 'Connect a wallet to get started.'}</p>
      ${account ? '<div class="mp-self-social"><a href="#/me/followers"><strong>—</strong> followers</a><span>·</span><a href="#/me/following"><strong>—</strong> following</a></div>' : ''}
      ${account ? `<div class="mp-self-identity"><span>Wallet identity</span><code title="${escape(account)}">${escape(short)}</code></div>` : ''}
      <div class="mp-self-actions"><a href="#/me/new">New repository</a><a href="#/me/repositories">My repositories</a><a href="#/portfolio">My memberships</a><a href="#/dev">Developer dashboard</a></div>
      <div class="mp-self-side-section"><h2>Achievements</h2><p>Not verified yet</p></div>
      <div class="mp-self-side-section"><h2>Organizations</h2><p>No signed organization links</p></div>
    </aside><div class="mp-self-main">${content}</div></div>
  </div>`;
}

function graph() {
  const days = Array.from({ length: 364 }, () => '<span class="mp-self-day" title="Signed activity data unavailable"></span>').join('');
  return `<div class="mp-self-graph" role="img" aria-label="Contribution graph: signed daily activity is unavailable">${days}</div><p class="mp-self-caption">Daily activity will appear here after a signed activity index is available. Empty cells do not mean zero contributions.</p>`;
}

function membershipCards(positions, error) {
  if (error) return empty('Memberships unavailable', 'The chain did not answer this wallet’s holdings. Try My memberships again when the node is reachable.');
  if (positions === null) return empty('Loading memberships', 'Reading this wallet’s holdings from the chain.');
  if (!positions.length) return empty('No memberships found', 'This wallet does not currently hold a model membership in the EVM namespace.');
  return `<div class="mp-self-cards">${positions.map((item) => `<article class="mp-self-card"><a href="${escape(item.href)}">${escape(item.name)}</a><span>${escape(item.units)} membership${item.units === '1' ? '' : 's'}</span></article>`).join('')}</div>`;
}

export function renderMyProfile({ main, account, tab = 'overview', positions = null, loading = false, error = false, onConnect }) {
  const active = tabs.some(([key]) => key === tab) ? tab : 'overview';
  let body;
  if (active === 'overview') body = `<section class="mp-self-readme"><div class="mp-self-readme-title">Profile / README.md</div><div class="mp-self-readme-content"><h2>${account ? 'Your profile README' : 'A profile for your MISAKA identity'}</h2><p>${account ? 'No signed profile README has been published for this wallet. A wallet address alone is not a claimed username or a public biography.' : 'Connect a wallet to view its memberships and developer tools. A public profile needs a signed content manifest and a chain-bound username.'}</p></div></section>`
    + panel('Pinned', empty('Nothing pinned yet', 'Signed profile content has not been published for this wallet.'))
    + panel('Contribution activity', graph())
    + panel('Memberships', account ? membershipCards(positions, error) : empty('Connect a wallet', 'Memberships are held by a wallet address.'), '<a href="#/portfolio">View all →</a>')
    + panel('Recent activity', empty('No signed activity feed', 'Browser transaction history remains available under My memberships. It is not a public contribution record.'));
  else if (active === 'memberships') body = panel('Your memberships', account ? membershipCards(positions, error) : empty('Connect a wallet', 'Memberships are held by a wallet address.'), '<a href="#/portfolio">Open full portfolio →</a>');
  else if (active === 'developer') body = panel('Developer dashboard', `<div class="mp-self-intro"><p>Manage model lines and stores associated with this wallet. PALW bond ownership is separate from an EVM wallet address; no model is attributed here without a verified identity link.</p><a class="btn" href="#/dev">Open developer dashboard →</a></div>`);
  else if (active === 'repositories') body = panel('Repositories', empty('No signed repositories linked', 'Repository metadata must come from a signed, content-addressed manifest. This wallet address alone does not prove GitHub ownership.'));
  else if (active === 'models') body = panel('Models', empty('No verified model identity linked', 'A model’s PALW owner bond must be linked to a signed profile key before it can be listed here.'));
  else if (active === 'stars') body = panel('Stars', empty('No verified stars available', 'Stars will come from signed social-event records.'));
  else body = panel(active === 'followers' ? 'Followers' : 'Following', empty('No verified social index', 'Signed follow events and a rebuildable index are required before identities or counts can be shown.'));
  const connect = !account ? panel('Connect a wallet', `<div class="mp-self-intro"><p>Connect your wallet to see its chain holdings and developer tools. No public identity will be inferred from the address.</p><button type="button" class="btn btn-accent" id="mpSelfConnect">Connect wallet</button></div>`) : '';
  main.innerHTML = wrapAccountWorkspace({ account, active, content: `${connect}${loading && account ? '<p class="mp-self-loading" role="status">Refreshing chain data…</p>' : ''}${body}` });
  main.querySelector('#mpSelfConnect')?.addEventListener('click', onConnect);
}
