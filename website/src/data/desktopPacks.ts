/**
 * Mac, Windows, office, developer-tool and browser packs for the /keep page. Mirrors the pack tables in docs/merion/SCENARIOS.md. Merion reads a file
 * the person exports; it does not connect to the machine or drive its desktop.
 */
export type DesktopPack = {id: string; os: 'macOS' | 'Windows' | 'Office' | 'Developer' | 'Browser' | 'Excel'; title: string; drop: string; get: string};

export const DESKTOP_PACKS: DesktopPack[] = [
  {id: 'mac-system-report', os: 'macOS', title: 'System report', drop: 'system_profiler output', get: 'Model, chip, memory, macOS version, protection flags, uptime'},
  {id: 'homebrew-audit', os: 'macOS', title: 'Homebrew audit', drop: 'brew list --versions', get: 'Package count, kept old versions, outdated packages, toolchains'},
  {id: 'mac-log-triage', os: 'macOS', title: 'Log triage', drop: 'log show, compact style', get: 'Error processes, repeated errors, sandbox denials, kernel trouble'},
  {id: 'mac-update-history', os: 'macOS', title: 'Update history', drop: 'softwareupdate --history', get: 'What was installed, versions, dates, betas'},
  {id: 'windows-systeminfo', os: 'Windows', title: 'System info', drop: 'systeminfo output', get: 'OS and build, boot time, memory, domain, hotfix KBs'},
  {id: 'windows-hotfixes', os: 'Windows', title: 'Hotfixes', drop: 'Get-HotFix as CSV', get: 'KB numbers, kinds of update, who installed, dates'},
  {id: 'windows-installed-software', os: 'Windows', title: 'Installed software', drop: 'An installed-programs CSV', get: 'Top publishers and programs, first rows'},
  {id: 'windows-event-log', os: 'Windows', title: 'Event log', drop: 'Get-WinEvent as CSV', get: 'Counts by level, provider and id; error and warning rows'},
  {id: 'receivables-ageing', os: 'Office', title: 'Receivables from mail', drop: 'A mail export', get: 'Invoice numbers, overdue and paid lines, due dates, amounts'},
  {id: 'po-line-items', os: 'Office', title: 'PO line items', drop: 'A purchase order as text', get: 'PO number, GSTINs, HSN codes, lines with amounts, open points'},
  {id: 'employee-ledger', os: 'Office', title: 'Employee ledger', drop: 'A monthly ledger CSV', get: 'Rows per employee and month, the first rows'},
  {id: 'reimbursement-claims', os: 'Office', title: 'Reimbursement claims', drop: 'A mail export', get: 'Claimants, amounts, approved and pending, categories'},
  {id: 'github-prs', os: 'Developer', title: 'GitHub pull requests', drop: 'gh pr list --json', get: 'States, authors, labels, merges per month, titles'},
  {id: 'github-issues', os: 'Developer', title: 'GitHub issues', drop: 'gh issue list --json', get: 'Open vs closed, authors, labels, issues per month'},
  {id: 'github-actions-log', os: 'Developer', title: 'Failed CI log', drop: 'gh run view --log-failed', get: 'Error lines, failing steps, exit codes, repeated errors'},
  {id: 'dependabot-alerts', os: 'Developer', title: 'Dependabot alerts', drop: 'The alerts JSON from gh api', get: 'Severities, packages, ecosystems, advisories'},
  {id: 'git-log-digest', os: 'Developer', title: 'Git log digest', drop: 'git log, pipe-separated', get: 'Commits per author and month, prefixes, merges'},
  {id: 'xcodebuild-log', os: 'Developer', title: 'Xcode build log', drop: 'xcodebuild output', get: 'Result, errors by file, repeated messages, failed tests'},
  {id: 'xcode-crash-log', os: 'Developer', title: 'Xcode crash report', drop: 'A legacy .crash report', get: 'App, OS, exception, crashed thread, frames'},
  {id: 'vscode-extensions', os: 'Developer', title: 'VS Code extensions', drop: 'code --list-extensions', get: 'Count, publishers, names'},
  {id: 'vscode-settings-audit', os: 'Developer', title: 'VS Code settings', drop: 'settings.json', get: 'Settings set, telemetry and trust lines, secret-looking names'},
  {id: 'mac-apps-inventory', os: 'macOS', title: 'Apps inventory', drop: 'system_profiler SPApplicationsDataType', get: 'Apps, where each came from, signer, kind'},
  {id: 'mac-launch-items', os: 'macOS', title: 'Launch items', drop: 'launchctl list', get: 'Non-Apple items, exit statuses'},
  {id: 'windows-services', os: 'Windows', title: 'Services', drop: 'Get-Service as CSV', get: 'Status and start-type counts, names'},
  {id: 'windows-scheduled-tasks', os: 'Windows', title: 'Scheduled tasks', drop: 'schtasks /query /fo csv /v', get: 'Tasks, state, last result, run-as account'},
  {id: 'bookmarks-digest', os: 'Browser', title: 'Bookmarks', drop: 'A bookmarks HTML export', get: 'Top sites, folders, titles'},
  {id: 'browser-history-takeout', os: 'Browser', title: 'Browser history', drop: 'Google Takeout BrowserHistory.json', get: 'Top sites, how pages were reached, titles'},
  {id: 'excel-sheets', os: 'Excel', title: 'Excel sheets', drop: 'A sales register, stock or attendance sheet', get: 'Rows per customer, location or employee; the first rows'},
];
