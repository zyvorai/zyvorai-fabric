// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

/**
 * Strings for the Keep console pages in English and Simplified Chinese.
 *
 * Partial on purpose: navigation, headings, buttons and status are translated. The long explanatory
 * paragraphs and the honesty notes stay in English so their meaning is never softened in translation;
 * a vendor building its own app will bring its own strings. A missing key falls back to English.
 */

export type Locale = 'en' | 'zh-CN'

const en = {
  'keep.title': 'Keep',
  'keep.history': 'History',
  'keep.agents': 'Agents',
  'keep.language': '中文',

  'home.run': 'Run {title}',
  'home.running': 'Running…',
  'home.pickFile': 'Pick a {accept} file',
  'home.pickFileOrSample': 'Pick a {accept} file (or use the sample)',
  'home.files': '{n} files',
  'home.openCockpit': 'Open cockpit',
  'home.newUseCase': 'New use case',
  'home.close': 'Close',
  'home.deployPack': 'Deploy a pack',
  'home.deployUseCaseTitle': 'Deploy your own use case',
  'home.deployPackTitle': 'Deploy an agent pack',
  'home.chip.cell': 'cell up',
  'home.chip.extract': 'extract',
  'home.chip.artifact': 'artifact',
  'home.connects': 'CONNECT from the cell',
  'home.sentTo': 'text sent to',
  'home.removeCustom': 'Remove this custom use case',
  'home.sendsText': 'Sends text out.',
  'home.batchDone': '{ok} of {count} done',
  'home.batchNote': 'one sealed cell per file',
  'home.done': 'done',
  'home.failed': 'failed',

  'hist.title': 'Keep history',
  'hist.tab.runs': 'Runs',
  'hist.tab.triggers': 'Triggers',
  'hist.tab.audit': 'Audit',
  'hist.tab.approvals': 'Approvals',
  'hist.useCase': 'Use case',
  'hist.all': 'All',
  'hist.compare': 'Compare selected',
  'hist.compareHint': 'Pick two runs to see what changed.',
  'hist.noRuns': 'No runs yet.',
  'hist.cockpit': 'cockpit',
  'hist.unchanged': '{n} unchanged',
  'hist.col.when': 'When',
  'hist.col.phase': 'Phase',
  'hist.col.action': 'Action',
  'hist.col.session': 'Session',
  'hist.filter.all': 'all',
  'hist.filter.pending': 'pending',
  'hist.filter.decided': 'decided',
  'hist.approvalsNote': "Read-only. Decide from the session's cockpit or your phone, never from this list.",
  'hist.kind': 'Kind',
  'hist.folderName': 'Folder name',
  'hist.addTrigger': 'Add trigger',
  'hist.remove': 'Remove',
  'hist.noTriggers': 'No triggers yet.',
  'hist.runsCount': '{n} runs',
  'hist.secretOnce': 'Copy the secret now. It is shown once and signs every call.',

  'session.detail': 'Session detail',
  'session.missing': 'Missing session',
  'session.missingHint': 'Open Keep from a session detail link.',
  'session.loadFailed': 'Could not load Keep view',
  'session.approve': 'Approve',
  'session.deny': 'Deny',
  'session.approved': 'Approved',
  'session.denied': 'Denied',
  'session.startCast': 'Start screencast',
  'session.stopCast': 'Stop screencast',
} as const

export type MerionKey = keyof typeof en

const zh: Record<MerionKey, string> = {
  'keep.title': 'Keep',
  'keep.history': '历史记录',
  'keep.agents': '智能体',
  'keep.language': 'English',

  'home.run': '运行 {title}',
  'home.running': '运行中…',
  'home.pickFile': '选择 {accept} 文件',
  'home.pickFileOrSample': '选择 {accept} 文件（或使用示例）',
  'home.files': '{n} 个文件',
  'home.openCockpit': '打开驾驶舱',
  'home.newUseCase': '新建用例',
  'home.close': '关闭',
  'home.deployPack': '部署包',
  'home.deployUseCaseTitle': '部署你自己的用例',
  'home.deployPackTitle': '部署智能体包',
  'home.chip.cell': '单元已启动',
  'home.chip.extract': '提取',
  'home.chip.artifact': '产物',
  'home.connects': '该单元的对外连接数',
  'home.sentTo': '文本已发送至',
  'home.removeCustom': '删除此自定义用例',
  'home.sendsText': '会向外发送文本。',
  'home.batchDone': '已完成 {ok} / {count}',
  'home.batchNote': '每个文件一个独立的密封单元',
  'home.done': '完成',
  'home.failed': '失败',

  'hist.title': 'Keep 历史记录',
  'hist.tab.runs': '运行',
  'hist.tab.triggers': '触发器',
  'hist.tab.audit': '审计',
  'hist.tab.approvals': '审批',
  'hist.useCase': '用例',
  'hist.all': '全部',
  'hist.compare': '对比所选',
  'hist.compareHint': '选择两次运行，查看有什么变化。',
  'hist.noRuns': '还没有运行记录。',
  'hist.cockpit': '驾驶舱',
  'hist.unchanged': '{n} 行未变',
  'hist.col.when': '时间',
  'hist.col.phase': '阶段',
  'hist.col.action': '操作',
  'hist.col.session': '会话',
  'hist.filter.all': '全部',
  'hist.filter.pending': '待处理',
  'hist.filter.decided': '已决定',
  'hist.approvalsNote': '只读。请在该会话的驾驶舱或你的手机上做决定，不要在此列表中操作。',
  'hist.kind': '类型',
  'hist.folderName': '文件夹名称',
  'hist.addTrigger': '添加触发器',
  'hist.remove': '移除',
  'hist.noTriggers': '还没有触发器。',
  'hist.runsCount': '{n} 次运行',
  'hist.secretOnce': '请立即复制密钥。它只显示这一次，并用于给每次调用签名。',

  'session.detail': '会话详情',
  'session.missing': '缺少会话',
  'session.missingHint': '请从会话详情链接打开 Keep。',
  'session.loadFailed': '无法加载 Keep 视图',
  'session.approve': '批准',
  'session.deny': '拒绝',
  'session.approved': '已批准',
  'session.denied': '已拒绝',
  'session.startCast': '开始屏幕直播',
  'session.stopCast': '停止屏幕直播',
}

export const DICTIONARIES: Record<Locale, Record<MerionKey, string>> = { en, 'zh-CN': zh }

const STORAGE_KEY = 'keep.locale'

/** The saved choice, else the browser's language. Storage can throw (private windows), so it is wrapped. */
export function detectLocale(): Locale {
  try {
    const saved = globalThis.localStorage?.getItem(STORAGE_KEY)
    if (saved === 'en' || saved === 'zh-CN') return saved
  } catch {
    /* storage unavailable */
  }
  const lang = (globalThis.navigator?.language ?? 'en').toLowerCase()
  return lang.startsWith('zh') ? 'zh-CN' : 'en'
}

export function saveLocale(locale: Locale): void {
  try {
    globalThis.localStorage?.setItem(STORAGE_KEY, locale)
  } catch {
    /* the choice just does not persist */
  }
}

/** Translate `key`, filling `{name}` placeholders. Falls back to English, then to the key itself. */
export function translate(
  locale: Locale,
  key: MerionKey,
  vars: Record<string, string | number> = {},
): string {
  const template = DICTIONARIES[locale][key] || DICTIONARIES.en[key] || key
  return template.replace(/\{(\w+)\}/g, (m, name: string) => (name in vars ? String(vars[name]) : m))
}
