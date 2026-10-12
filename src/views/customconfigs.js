// =============================================================================
//  src/views/customconfigs.js — زیرصفحهٔ «کانفیگ‌های سفارشی» (۱.۳.۰)
//  پورت از ui/settings/CustomConfigsPage.kt نسخهٔ اندروید ۱.۵.۰ — بدونِ QR.
// =============================================================================
//
//  # آنچه این صفحه هرگز ندارد
//
//  نمایی که از Rust می‌آید (`cc_state`) outbound و متنِ خامِ کانفیگ را ندارد.
//  متنِ خام فقط با `cc_get_raw` و فقط برای کانفیگِ قفل‌نشده خوانده می‌شود (ویرایش
//  و کپیِ لینک). کانفیگِ قفل‌شدهٔ npvt نشانی و کلیدش را هیچ‌جا نشان نمی‌دهد.
//
//  # QR
//
//  نسخهٔ ویندوز دوربین و اسکنرِ تصویر ندارد؛ ورود فقط از متن، نشانیِ اشتراک،
//  انتخابِ فایل و کشیدن‌ورها‌کردنِ فایل است.

import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { t, getLang } from '../i18n.js'
import { toast } from '../ui/toast.js'
import { emit } from '../main.js'
import { openModal, modalButton, modalNote, confirmModal } from '../ui/modal.js'

const EMPTY = { enabled: false, mode: 'SOLO', modeLabel: 'Config', activeId: null, modes: [], configs: [], max: 500 }

/** آخرین نمای شناخته‌شده؛ ردیفِ هاب مقدارش را از همین می‌خواند. */
let CC = { ...EMPTY }
const watchers = new Set()
let wired = false

const FILE_ACCEPT = '.npvt,.txt,.json,.conf'

// ------------------------------------------------------------- real delay
//
//  «پینگِ واقعی» (`cc_ping` / `cc_ping_all`): هر کانفیگ در یک Xrayِ موقت اجرا و
//  یک `GET /generate_204` از دلِ آن فرستاده می‌شود. نتیجه فقط در حافظهٔ همین
//  صفحه می‌ماند (برای هر شناسه): `{ ms, status, label }` یا `{ pending: true }`.

const PING = new Map()
let pingAllBusy = false

/** برچسب‌های وضعیت؛ رشتهٔ انگلیسی همان است که Rust می‌فرستد. */
const PING_LABELS = ['No response', 'Server off', 'Filtered', 'Invalid config', 'No network']

export function pingText(r) {
  if (!r) return ''
  if (r.pending) return t('Testing…')
  if (typeof r.ms === 'number' && r.ms >= 0) return `${r.ms} ms`
  const label = PING_LABELS.includes(r.label) ? t(r.label) : t('No response')
  return `-1 · ${label}`
}

function pingClass(r) {
  if (!r) return ''
  if (r.pending) return 'is-wait'
  if (r.ms < 0) return 'is-bad'
  return r.ms <= 800 ? 'is-good' : 'is-slow'
}

function notifyWatchers() {
  for (const fn of [...watchers]) fn()
}

function adoptPings(payload) {
  for (const r of payload?.results ?? []) {
    if (r && r.id) PING.set(r.id, { ms: r.ms, status: r.status, label: r.label })
  }
  notifyWatchers()
}

async function pingOne(c) {
  if (PING.get(c.id)?.pending) return
  PING.set(c.id, { pending: true })
  notifyWatchers()
  try {
    adoptPings(await invoke('cc_ping', { id: c.id }))
  } catch (e) {
    PING.delete(c.id)
    notifyWatchers()
    toast(errorText(e))
  }
}

async function pingAll() {
  if (pingAllBusy || !CC.configs.length) return
  pingAllBusy = true
  for (const c of CC.configs) PING.set(c.id, { pending: true })
  notifyWatchers()
  try {
    adoptPings(await invoke('cc_ping_all'))
  } catch (e) {
    for (const c of CC.configs) if (PING.get(c.id)?.pending) PING.delete(c.id)
    toast(errorText(e))
  } finally {
    pingAllBusy = false
    notifyWatchers()
  }
}

// ------------------------------------------------------------- paste hygiene
//
//  همان فهرستِ `ConfigParser.cleanInput` اندروید (و `import::clean_input` در
//  Rust): RLM/LRM و جداکننده‌های bidi که پیام‌رسان‌های راست‌به‌چپ دورِ متنِ لاتین
//  می‌گذارند، فاصلهٔ صفرعرض، word joiner، BOM، soft hyphen. ZWNJ/ZWJ می‌مانند.
// eslint-disable-next-line no-misleading-character-class
const INVISIBLE = /[\u00AD\u061C\u180E\u200B\u200E\u200F\u202A-\u202E\u2060-\u2064\u2066-\u2069\uFEFF]/g

export function stripInvisible(text) {
  return String(text ?? '')
    .replace(/\r\n?/g, '\n')
    .replace(INVISIBLE, '')
    .replace(/[\u00A0\u202F]/g, ' ')
}

function esc(v) {
  return String(v ?? '').replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;').replace(/'/g, '&#39;')
}

function adopt(view) {
  if (view && typeof view === 'object' && Array.isArray(view.configs)) CC = { ...EMPTY, ...view }
  for (const fn of [...watchers]) fn()
  // ردیفِ هابِ تنظیمات مقدارش را از `CC` می‌خواند.
  emit()
}

function wireOnce() {
  if (wired) return
  wired = true
  try {
    Promise.resolve(listen('aether://custom', (e) => adopt(e.payload))).catch(() => {})
    Promise.resolve(listen('aether://custom-ping', (e) => adoptPings(e.payload))).catch(() => {})
  } catch (_) { /* jsdom */ }
  wireDrop()
}

// ------------------------------------------------------------- drag & drop
//
//  سه مسیر، با یک مقصد (`importPaths` / `importFiles`):
//
//  ۱. رویدادهای بومیِ تائوری (`tauri://drag-enter|over|drop|leave`) — وقتی برنامه
//     بی‌ارتقا اجرا شود. با `listen` سراسری گوش داده می‌شوند (هدفِ Any)، تا فرقی
//     نکند Rust آن‌ها را به webview بفرستد یا به window.
//  ۲. `aether://file-drop` از `filedrop.rs` — برنامهٔ نصب‌شده با دسترسیِ مدیر اجرا
//     می‌شود و ویندوز (UIPI) رها کردنِ OLE از Explorer را به آن نمی‌رساند؛ Rust
//     همان مسیرها را از `WM_DROPFILES` می‌خواند.
//  ۳. رویدادهای HTML5 (`dragover`/`drop`) با `File`ِ واقعی — اگر WebView خودش
//     فایل را تحویل داد. بدونِ `preventDefault` روی dragover، رها کردن مجاز نیست
//     و WebView به‌جایش فایل را باز می‌کند.
//
//  یک رها کردن ممکن است از دو مسیر برسد؛ نامِ فایل تا دو ثانیه یک بار پذیرفته می‌شود.

const DROP_DEDUPE_MS = 2000
const recentDrops = new Map()
let dropWired = false

function baseName(p) {
  return String(p ?? '').split(/[\\/]/).pop().toLowerCase()
}

function freshDrop(name) {
  const now = Date.now()
  for (const [k, at] of recentDrops) if (now - at > DROP_DEDUPE_MS) recentDrops.delete(k)
  if (!name || recentDrops.has(name)) return false
  recentDrops.set(name, now)
  return true
}

function dropZone() {
  return document.querySelector('.cc-drop')
}

function setDropOver(on) {
  dropZone()?.classList.toggle('is-over', !!on)
}

/** مسیرهایی که از رویدادِ پنجره رسیده‌اند؛ تعدادِ پذیرفته‌شده. */
export function handleDroppedPaths(paths) {
  setDropOver(false)
  if (!dropZone()) return 0
  const list = (Array.isArray(paths) ? paths : [])
    .filter((p) => typeof p === 'string' && p.trim())
    .filter((p) => freshDrop(baseName(p)))
  if (list.length) importPaths(list)
  return list.length
}

/** `File`هایی که خودِ WebView تحویل داده؛ تعدادِ پذیرفته‌شده. */
export function handleDroppedFiles(files) {
  setDropOver(false)
  if (!dropZone()) return 0
  const list = [...(files ?? [])].filter((f) => f && freshDrop(String(f.name ?? '').toLowerCase()))
  if (list.length) importFiles(list)
  return list.length
}

function hasFiles(ev) {
  const types = ev?.dataTransfer?.types
  if (!types) return false
  return typeof types.includes === 'function' ? types.includes('Files') : [...types].includes('Files')
}

function wireDrop() {
  if (dropWired) return
  dropWired = true
  const onTauri = (name, fn) => {
    try { Promise.resolve(listen(name, fn)).catch(() => {}) } catch (_) { /* jsdom */ }
  }
  onTauri('tauri://drag-enter', () => setDropOver(true))
  onTauri('tauri://drag-over', () => setDropOver(true))
  onTauri('tauri://drag-leave', () => setDropOver(false))
  onTauri('tauri://drag-drop', (e) => handleDroppedPaths(e?.payload?.paths))
  onTauri('aether://file-drop', (e) => handleDroppedPaths(Array.isArray(e?.payload) ? e.payload : e?.payload?.paths))

  const allow = (ev) => {
    if (!hasFiles(ev)) return
    ev.preventDefault()
    const zone = dropZone()
    if (ev.dataTransfer) ev.dataTransfer.dropEffect = zone ? 'copy' : 'none'
    setDropOver(!!zone)
  }
  document.addEventListener('dragenter', allow)
  document.addEventListener('dragover', allow)
  document.addEventListener('dragleave', (ev) => {
    // فقط وقتی نشانگر واقعاً از پنجره بیرون رفت، نه بینِ دو عنصر.
    if (!ev.relatedTarget) setDropOver(false)
  })
  document.addEventListener('drop', (ev) => {
    if (!hasFiles(ev)) return
    ev.preventDefault()
    handleDroppedFiles(ev.dataTransfer?.files)
  })
}

/** برای ردیفِ هاب. */
export function customConfigsValue() {
  wireOnce()
  if (!CC.enabled) return t('Off')
  return CC.modeLabel ? chainText(CC.modeLabel) : t('On')
}

/** بارِ اول (و پس از هر بازگشت به هاب) نما را از Rust می‌خواند. */
export async function refreshCustomConfigs() {
  wireOnce()
  try {
    adopt(await invoke('cc_state'))
  } catch (_) {
    adopt(null)
  }
}

export function onCustomConfigs(fn) {
  watchers.add(fn)
  return () => watchers.delete(fn)
}

// ------------------------------------------------------------- errors

function errorText(e) {
  if (!e) return t('Something went wrong.')
  if (typeof e === 'string') return t(e)
  if (e.kind === 'password') return t('This npvt config is password protected.')
  if (e.kind === 'wrongPassword') return t('Wrong password for this npvt config.')
  if (e.kind === 'locked') return t('This config is locked by its author.')
  return t(e.message || String(e))
}

/** دیالوگِ رمز؛ `null` اگر کاربر لغو کرد. */
function askPassword(wrong) {
  return new Promise((resolve) => {
    let done = false
    const finish = (v) => { if (done) return; done = true; modal.close(); resolve(v) }
    const modal = openModal({ title: t('Password'), onEscape: () => finish(null) })
    modal.body.append(modalNote(wrong ? t('Wrong password for this npvt config.') : t('This npvt config is password protected.'), wrong ? 'is-error' : ''))
    const input = document.createElement('input')
    input.className = 'input ltr'
    input.dir = 'ltr'
    input.type = 'password'
    input.autocomplete = 'off'
    input.placeholder = t('Password')
    input.addEventListener('keydown', (ev) => { if (ev.key === 'Enter') { ev.preventDefault(); finish(input.value) } })
    modal.body.append(input)
    modal.actions.append(
      modalButton(t('Cancel'), 'btn--ghost', () => finish(null)),
      modalButton(t('Unlock'), 'btn--primary', () => finish(input.value)),
    )
    setTimeout(() => input.focus(), 0)
  })
}

/** یک ورود؛ اگر رمز لازم شد می‌پرسد و دوباره می‌فرستد. */
async function withPassword(call) {
  let password = null
  for (let round = 0; round < 4; round++) {
    try {
      const view = await call(password)
      adopt(view)
      const n = view?.added ?? 0
      if (n) toast(n === 1 ? t('1 config imported.') : t('{0} configs imported.').replace('{0}', String(n)))
      return true
    } catch (e) {
      if (e && (e.kind === 'password' || e.kind === 'wrongPassword')) {
        password = await askPassword(e.kind === 'wrongPassword')
        if (password === null) return false
        continue
      }
      toast(errorText(e))
      return false
    }
  }
  return false
}

function importPaths(paths) {
  ;(async () => {
    for (const path of paths) {
      await withPassword((password) => invoke('cc_import_path', { path, password }))
    }
  })()
}

async function importFiles(files) {
  for (const file of files) {
    let text
    try { text = await readFile(file) } catch (e) { toast(errorText(e)); continue }
    await withPassword((password) => invoke('cc_import_file', { name: file.name, text, password }))
  }
}

function readFile(file) {
  return new Promise((resolve, reject) => {
    if (file.size > 2 * 1024 * 1024) { reject(t('file is larger than 2 MB')); return }
    const r = new FileReader()
    r.onload = () => resolve(String(r.result ?? ''))
    r.onerror = () => reject(t('file could not be read'))
    r.readAsText(file)
  })
}

// ------------------------------------------------------------- dialogs

function textDialog({ title, value = '', area = false, okLabel, note = '' }) {
  return new Promise((resolve) => {
    let done = false
    const finish = (v) => { if (done) return; done = true; modal.close(); resolve(v) }
    const modal = openModal({ title, onEscape: () => finish(null) })
    if (note) modal.body.append(modalNote(note))
    const input = document.createElement(area ? 'textarea' : 'input')
    input.className = area ? 'input input--area ltr cc-edit' : 'input'
    if (area) input.dir = 'ltr'
    input.value = value
    if (!area) input.addEventListener('keydown', (ev) => { if (ev.key === 'Enter') { ev.preventDefault(); finish(input.value) } })
    modal.body.append(input)
    modal.actions.append(
      modalButton(t('Cancel'), 'btn--ghost', () => finish(null)),
      modalButton(okLabel, 'btn--primary', () => finish(input.value)),
    )
    setTimeout(() => input.focus(), 0)
  })
}

async function editConfig(c) {
  let raw
  try { raw = await invoke('cc_get_raw', { id: c.id }) } catch (e) { toast(errorText(e)); return }
  const next = await textDialog({ title: t('Edit config'), value: raw ?? '', area: true, okLabel: t('Save') })
  if (next === null) return
  await withPassword((password) => invoke('cc_edit', { id: c.id, raw: next, password }))
}

async function renameConfig(c) {
  const next = await textDialog({ title: t('Rename'), value: c.name, okLabel: t('Save') })
  if (next === null || !next.trim()) return
  adopt(await invoke('cc_rename', { id: c.id, name: next }))
}

async function copyLink(c) {
  try {
    const raw = await invoke('cc_get_raw', { id: c.id })
    await navigator.clipboard.writeText(raw ?? '')
    toast(t('Copied'))
  } catch (e) {
    toast(errorText(e))
  }
}

async function deleteConfig(c) {
  const ok = await confirmModal({
    title: t('Delete this config?'),
    body: c.name,
    ok: t('Delete'),
    cancel: t('Cancel'),
    danger: true,
  })
  if (ok) adopt(await invoke('cc_delete', { id: c.id }))
}

async function deleteAll() {
  const ok = await confirmModal({
    title: t('Delete all configs?'),
    body: t('Every imported config is removed from this PC. This cannot be undone.'),
    ok: t('Delete all'),
    cancel: t('Cancel'),
    danger: true,
  })
  if (ok) adopt(await invoke('cc_delete_all'))
}

// ------------------------------------------------------------- render

const ROUTE_ICON =
  '<svg viewBox="0 0 24 24" width="19" height="19" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"><circle cx="6" cy="18" r="2.2"/><circle cx="18" cy="6" r="2.2"/><path d="M8.2 18H15a3 3 0 0 0 0-6H9a3 3 0 0 1 0-6h6.8"/></svg>'
const CHEV_ICON =
  '<svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M9 5l7 7-7 7"/></svg>'
const CHECK_ICON =
  '<svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><path d="M5 12.5l4.5 4.5L19 7.5"/></svg>'

/** «Config» عمومی (پیش از انتخابِ کانفیگ) ترجمه می‌شود؛ نامِ پروتکل‌ها (VLESS، Npvt…) در هر دو زبان انگلیسی می‌مانند. */
function chainText(label) {
  return String(label ?? '').replace(/\bConfig\b/g, t('Config'))
}

/** زیرنویسِ هر زنجیره — `hopsLabel` موبایل. */
function hopsText(hops) {
  if (hops === 1) return t('Single · the config alone')
  return hops === 2 ? t('Double chain') : t('Triple chain')
}

function expiryText(c) {
  if (!c.expiresAt) return ''
  const d = new Date(c.expiresAt)
  const when = d.toLocaleDateString(getLang() === 'fa' ? 'fa-IR' : 'en-GB', { year: 'numeric', month: 'short', day: 'numeric' })
  return c.expired ? t('Expired on {0}').replace('{0}', when) : t('Expires on {0}').replace('{0}', when)
}

function configRow(c, activeId) {
  const row = document.createElement('div')
  row.className = `ccfg ${c.id === activeId ? 'is-active' : ''} ${c.expired ? 'is-expired' : ''}`.trim()
  row.setAttribute('role', 'radio')
  row.setAttribute('aria-checked', String(c.id === activeId))
  row.tabIndex = 0
  const ping = PING.get(c.id)
  const badges = [
    `<span class="ccfg__badge ltr" dir="ltr">${esc(c.chainLabel)}</span>`,
    ping ? `<span class="ccfg__ping ${pingClass(ping)}" dir="auto">${esc(pingText(ping))}</span>` : '',
    c.locked ? `<span class="ccfg__badge ccfg__badge--lock">${esc(t('Locked'))}</span>` : '',
    c.expired ? `<span class="ccfg__badge ccfg__badge--bad">${esc(t('Expired'))}</span>` : '',
  ].join('')
  row.innerHTML = `
    <span class="ccfg__dot" aria-hidden="true">${CHECK_ICON}</span>
    <span class="ccfg__text">
      <span class="ccfg__name" dir="auto">${esc(c.name)}</span>
      <span class="ccfg__meta">${badges}${c.server ? `<span class="ccfg__server ltr" dir="ltr">${esc(c.server)}</span>` : ''}</span>
      ${c.note ? `<span class="ccfg__note" dir="auto">${esc(c.note)}</span>` : ''}
      ${c.expiresAt ? `<span class="ccfg__note">${esc(expiryText(c))}</span>` : ''}
    </span>
    <span class="ccfg__actions"></span>`
  const actions = row.querySelector('.ccfg__actions')
  const act = (text, fn, kind = '') => {
    const b = document.createElement('button')
    b.type = 'button'
    b.className = `btn btn--small btn--ghost ${kind}`.trim()
    b.textContent = text
    b.addEventListener('click', (ev) => { ev.stopPropagation(); fn(c) })
    actions.appendChild(b)
  }
  act(t('Ping'), pingOne, 'ccfg__ping-btn')
  if (!c.locked) act(t('Edit'), editConfig)
  act(t('Rename'), renameConfig)
  if (c.shareable) act(t('Copy link'), copyLink)
  act(t('Delete'), deleteConfig, 'btn--danger')
  const pick = async () => { adopt(await invoke('cc_select', { id: c.id })) }
  row.addEventListener('click', pick)
  row.addEventListener('keydown', (ev) => {
    if (ev.target === row && (ev.key === 'Enter' || ev.key === ' ')) { ev.preventDefault(); pick() }
  })
  return row
}

/** کلِ پنل؛ `settings.js` آن را در زیرصفحهٔ عمومی می‌نشاند. */
export function renderCustomConfigs() {
  wireOnce()
  const root = document.createElement('div')
  root.className = 'view view--advanced view--custom'

  root.innerHTML = `
    <div class="scard-group">
      <h3 class="sethub__group scard__caption">${esc(t('Custom config'))}</h3>
      <div class="sethub__card scard scard--fields">
        <section class="field field--row">
          <div>
            <span class="field__label">${esc(t('Use a custom config'))}</span>
            <span class="field__hint cc-enable-hint"></span>
          </div>
          <button type="button" class="switch cc-enable" role="switch" aria-checked="false"><span class="switch__knob"></span></button>
        </section>
        <section class="field field--pick cc-chainpick">
          <div class="pick cc-pick" role="button" tabindex="0" aria-haspopup="dialog">
            <span class="pick__icon">${ROUTE_ICON}</span>
            <span class="pick__text">
              <span class="field__label">${esc(t('Chain'))}</span>
              <span class="field__hint">${esc(t('Where the config sits in the route. Config first: the engine dials out through it. Config last: it dials out through the engine. Changes apply at the next connection.'))}</span>
            </span>
            <span class="pick__value cc-pick__value"><span class="pick__value-text cc-chain ltr" dir="ltr"></span><span class="cc-pick__hops"></span></span>
            <span class="pick__chev">${CHEV_ICON}</span>
          </div>
          <div class="sheet cc-sheet" hidden>
            <div class="sheet__scrim"></div>
            <div class="sheet__panel" role="dialog" aria-modal="true" aria-label="${esc(t('Chain'))}">
              <span class="sheet__handle" aria-hidden="true"></span>
              <h3 class="sheet__title">${esc(t('Chain'))}</h3>
              <div class="seg seg--sheet seg--chain cc-modes" role="radiogroup"></div>
            </div>
          </div>
        </section>
      </div>
    </div>

    <div class="scard-group">
      <h3 class="sethub__group scard__caption">${esc(t('Import'))}</h3>
      <div class="sethub__card scard scard--fields">
        <section class="field">
          <span class="field__label">${esc(t('Link, subscription or config'))}</span>
          <textarea class="input input--area ltr cc-paste" dir="ltr" placeholder="vless://… · vmess://… · trojan://… · ss://… · socks://… · wireguard://… · https://…/sub · npvt"></textarea>
          <span class="field__hint">${esc(t('Paste one or more share links, a full Xray JSON config, a base64 subscription, an npvt config, or a single https:// subscription address.'))}</span>
          <div class="cc-buttons">
            <button type="button" class="btn btn--primary btn--small cc-import">${esc(t('Import'))}</button>
            <button type="button" class="btn btn--small cc-file">${esc(t('Choose file…'))}</button>
            <input type="file" class="cc-file-input" accept="${FILE_ACCEPT}" multiple hidden>
          </div>
          <div class="cc-drop" role="note">${esc(t('…or drop a .npvt, .txt, .json or .conf file on this window.'))}</div>
        </section>
      </div>
      <p class="scard__footer">${esc(t('QR codes are not supported on Windows (no camera or image scanner). Use a link, a subscription or a file instead.'))}</p>
    </div>

    <div class="scard-group">
      <div class="cc-list-head">
        <h3 class="sethub__group scard__caption cc-list-title"></h3>
        <button type="button" class="btn btn--small btn--ghost cc-ping-all">${esc(t('Test all'))}</button>
      </div>
      <div class="sethub__card scard cc-list"></div>
    </div>

    <div class="scard-group cc-danger-group">
      <div class="sethub__card sethub__card--danger">
        <div class="setrow setrow--danger cc-delete-all" role="button" tabindex="0">
          <span class="setrow__icon"><svg viewBox="0 0 24 24" width="19" height="19" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round"><path d="M4 7h16M10 11v6M14 11v6M6 7l1 12h10l1-12M9 7V4h6v3"/></svg></span>
          <span class="setrow__text"><span class="setrow__title">${esc(t('Delete all configs'))}</span><span class="setrow__sub">${esc(t('Removes every imported config from this PC.'))}</span></span>
        </div>
      </div>
    </div>`

  const sw = root.querySelector('.cc-enable')
  const swHint = root.querySelector('.cc-enable-hint')
  const pickRow = root.querySelector('.cc-pick')
  const sheet = root.querySelector('.cc-sheet')
  const modesBox = root.querySelector('.cc-modes')
  const chain = root.querySelector('.cc-chain')
  const chainHops = root.querySelector('.cc-pick__hops')
  let modesSig = ''
  const list = root.querySelector('.cc-list')
  const listTitle = root.querySelector('.cc-list-title')
  const paste = root.querySelector('.cc-paste')
  const fileInput = root.querySelector('.cc-file-input')
  const danger = root.querySelector('.cc-danger-group')
  const pingAllBtn = root.querySelector('.cc-ping-all')

  const sync = () => {
    const has = !!CC.activeId
    sw.classList.toggle('is-on', !!CC.enabled)
    sw.setAttribute('aria-checked', String(!!CC.enabled))
    sw.disabled = !has && !CC.enabled
    swHint.textContent = has
      ? t('Off: Aether runs exactly as before. On: the next connection runs the chain below.')
      : t('Import a config first, then pick it in the list.')
    // برچسبِ هر زنجیره نامِ پروتکلِ کانفیگِ انتخاب‌شده را دارد (`VLESS → Tor`،
    // `Npvt → Aether`) — همان `chainLabel` موبایل. فقط وقتی هنوز کانفیگی انتخاب
    // نشده، Rust واژهٔ عمومیِ «Config» را می‌فرستد که ترجمه می‌شود.
    const sig = `${getLang()}|${CC.mode}|${CC.modes.map((m) => `${m.id}=${m.label}`).join(',')}`
    if (sig !== modesSig) {
      modesSig = sig
      modesBox.innerHTML = CC.modes.map((m) => {
        const on = m.id === CC.mode
        return `<button type="button" role="radio" class="seg__item cc-opt ${on ? 'is-active' : ''}" data-value="${esc(m.id)}" aria-checked="${on}">
            <span class="seg__text"><span class="cc-opt__title ltr" dir="ltr">${esc(chainText(m.label))}</span><span class="cc-opt__sub">${esc(hopsText(m.hops))}</span></span>
            <span class="seg__check">${CHECK_ICON}</span>
          </button>`
      }).join('')
    }
    const current = CC.modes.find((m) => m.id === CC.mode)
    chain.textContent = chainText(CC.modeLabel || current?.label || '')
    chainHops.textContent = current ? hopsText(current.hops) : ''
    listTitle.textContent = `${t('Saved configs')} (${CC.configs.length})`
    list.replaceChildren(...(CC.configs.length
      ? CC.configs.map((c) => configRow(c, CC.activeId))
      : [Object.assign(document.createElement('p'), { className: 'cc-empty', textContent: t('No configs yet.') })]))
    danger.hidden = CC.configs.length === 0
    pingAllBtn.hidden = CC.configs.length === 0
    pingAllBtn.disabled = pingAllBusy
    pingAllBtn.textContent = pingAllBusy ? t('Testing…') : t('Test all')
  }
  // صفحه‌ای که کاربر از آن بیرون رفته (از DOM جدا شده) دیگر گوش نمی‌دهد.
  const unwatch = onCustomConfigs(() => {
    if (!root.isConnected) { unwatch(); return }
    sync()
  })

  sw.addEventListener('click', async () => {
    adopt(await invoke('cc_set_enabled', { enabled: !CC.enabled }))
  })
  // برگهٔ پایینیِ «زنجیره» — همان `SettingsChoiceRow` موبایل (a4): کارت‌های گرد،
  // عنوانِ زنجیره + زیرنویسِ تکی/دوتایی/سه‌تایی، انتخاب‌شده با قابِ آبی و تیک.
  const sheetKeys = (e) => {
    const items = [...modesBox.querySelectorAll('.seg__item')]
    if (e.key === 'Escape') { e.preventDefault(); closeSheet(); pickRow.focus() }
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      e.preventDefault()
      const at = items.indexOf(document.activeElement)
      const next = at === -1 ? 0 : (at + (e.key === 'ArrowDown' ? 1 : -1) + items.length) % items.length
      items[next]?.focus()
    }
  }
  const openSheet = () => {
    if (!sheet.hidden) return
    sheet.hidden = false
    requestAnimationFrame(() => sheet.classList.add('is-open'))
    document.addEventListener('keydown', sheetKeys, true)
    const items = [...modesBox.querySelectorAll('.seg__item')]
    ;(items.find((i) => i.classList.contains('is-active')) ?? items[0])?.focus()
  }
  function closeSheet() {
    if (sheet.hidden) return
    sheet.classList.remove('is-open')
    sheet.hidden = true
    document.removeEventListener('keydown', sheetKeys, true)
  }
  pickRow.addEventListener('click', openSheet)
  pickRow.addEventListener('keydown', (e) => {
    if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); openSheet() }
  })
  sheet.querySelector('.sheet__scrim').addEventListener('click', closeSheet)
  modesBox.addEventListener('click', async (e) => {
    const item = e.target.closest('.seg__item')
    if (!item || !modesBox.contains(item)) return
    closeSheet()
    pickRow.focus()
    if (item.dataset.value !== CC.mode) adopt(await invoke('cc_set_mode', { mode: item.dataset.value }))
  })
  pingAllBtn.addEventListener('click', pingAll)
  // چسباندن: نویسه‌های نامرئی همان لحظه می‌روند تا کاربر لینکِ واقعی را ببیند.
  paste.addEventListener('paste', (ev) => {
    const raw = ev.clipboardData?.getData('text')
    if (typeof raw !== 'string') return
    const clean = stripInvisible(raw)
    if (clean === raw) return
    ev.preventDefault()
    const { selectionStart: a = paste.value.length, selectionEnd: b = paste.value.length } = paste
    paste.value = paste.value.slice(0, a) + clean + paste.value.slice(b)
    paste.selectionStart = paste.selectionEnd = a + clean.length
  })
  root.querySelector('.cc-import').addEventListener('click', async () => {
    const text = stripInvisible(paste.value).trim()
    if (!text) { toast(t('Paste a link or config first.')); return }
    const ok = await withPassword((password) => invoke('cc_import_text', { text, password }))
    if (ok) paste.value = ''
  })
  root.querySelector('.cc-file').addEventListener('click', () => fileInput.click())
  fileInput.addEventListener('change', async () => {
    await importFiles([...(fileInput.files ?? [])])
    fileInput.value = ''
  })
  const delAll = root.querySelector('.cc-delete-all')
  delAll.addEventListener('click', deleteAll)
  delAll.addEventListener('keydown', (ev) => {
    if (ev.key === 'Enter' || ev.key === ' ') { ev.preventDefault(); deleteAll() }
  })

  sync()
  // پس از نشستنِ پنل در DOM (همین تیک) نمای تازه از Rust می‌آید.
  setTimeout(refreshCustomConfigs, 0)
  return root
}
