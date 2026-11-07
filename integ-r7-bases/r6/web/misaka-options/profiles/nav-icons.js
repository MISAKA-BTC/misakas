/* Small, decorative profile-tab icons. Labels remain the accessible names. */
const paths = {
  overview: '<path d="M2 4.75C2 3.78 2.78 3 3.75 3h4.5c1.36 0 2.67.43 3.75 1.24A6.12 6.12 0 0 1 15.75 3h4.5c.97 0 1.75.78 1.75 1.75V19a1 1 0 0 1-1 1h-5.25A5.2 5.2 0 0 0 12 21.5 5.2 5.2 0 0 0 8.25 20H3a1 1 0 0 1-1-1V4.75ZM12 5v15m0-15c-1.03-1-2.33-1.5-3.75-1.5h-4.5v15h4.5c1.42 0 2.72.5 3.75 1.5m0-15c1.03-1 2.33-1.5 3.75-1.5h4.5v15h-4.5c-1.42 0-2.72.5-3.75 1.5"/>',
  repositories: '<path d="M5 2.75h12A2.25 2.25 0 0 1 19.25 5v14.25H6A2.25 2.25 0 0 1 3.75 17V5A2.25 2.25 0 0 1 6 2.75ZM5 17a2.25 2.25 0 0 1 2.25-2.25h12m-12 0V4.25m8.5 10.5V21l-3-2-3 2v-6.25"/>',
  models: '<path d="m12 2 8.5 5v10L12 22l-8.5-5V7L12 2Zm0 0v10m8.5-5L12 12 3.5 7M12 12v10"/>',
  stars: '<path d="m12 2.5 3.05 6.18 6.83.99-4.94 4.82 1.17 6.8L12 18.08l-6.11 3.21 1.17-6.8-4.94-4.82 6.83-.99L12 2.5Z"/>',
  followers: '<circle cx="9" cy="8" r="3"/><path d="M2.75 20v-1.5a6.25 6.25 0 0 1 12.5 0V20H2.75Zm13-15a3 3 0 0 1 0 6m2.5 3a5 5 0 0 1 3 4.5V20h-3"/>',
  following: '<circle cx="9" cy="8" r="3"/><path d="M2.75 20v-1.5a6.25 6.25 0 0 1 12.5 0V20H2.75Zm14-12h5m-2.5-2.5v5"/>',
  memberships: '<rect x="3" y="5" width="18" height="14" rx="2"/><path d="M3 10h18m-14 5h4"/>',
  developer: '<path d="m8 5-6 7 6 7m8-14 6 7-6 7M14 3l-4 18"/>',
  projects: '<rect x="3" y="3" width="18" height="18" rx="2"/><path d="M3 11h18M11 3v18"/>',
  packages: '<path d="m12 2 8.5 5v10L12 22l-8.5-5V7L12 2Zm0 0v10m8.5-5L12 12 3.5 7M12 12v10"/>',
};

export function profileTabIcon(tab) {
  const path = paths[tab] || paths.overview;
  return `<svg class="mp-tab-icon" aria-hidden="true" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round">${path}</svg>`;
}
