// ==========================================================================
// Batch Helper - Main Application Logic (Win95 Multi-Tab Edition)
// ==========================================================================

import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';

// Multi-Project Tabs State
let projectTabs = [];
let activeTabId = '';
let tabCounter = 0;

let historyListCache = [];

let envData = null;
let antScanData = null;
let jdkScanData = null;

let selectedAntPath = null;
let selectedAntRootOrZip = null;
let selectedJavaHome = null;

let userSetting = {
  ps_window_width: 1000,
  ps_window_height: 650,
  auto_capture_clipboard: true,
  auto_save_image: false,
  screenshot_save_dir: null,
};

let defaultScreenshotRootDir = '';
let batSplitLeftPct = 40;
let batViewerDarkTheme = false;
let dbPasswordVisible = false;
let groupRunSteps = [];
let groupRunSelectedIdx = 0;
let activeGroupRunLocked = false;
let runningGroupId = null;
let addGroupPendingTabId = '';
let addGroupPendingFiles = [];
let batListActive = false;

try {
  batViewerDarkTheme = localStorage.getItem('bh_bat_viewer_theme') === 'dark';
} catch (_) {
  batViewerDarkTheme = false;
}

function escapeHtml(str) {
  return String(str || '')
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;');
}

function isModalVisible(modalId) {
  const el = document.getElementById(modalId);
  return !!el && el.style.display !== 'none';
}

function closeAllRegularModals() {
  const modalIds = [
    'modal-ant',
    'modal-java',
    'modal-db',
    'modal-settings',
    'modal-auto-config',
    'modal-add-group',
    'modal-confirm-clear-log',
    'modal-target-path-warning',
  ];
  modalIds.forEach((id) => {
    const el = document.getElementById(id);
    if (el) el.style.display = 'none';
  });
  if (!activeGroupRunLocked) {
    const gr = document.getElementById('modal-group-run');
    if (gr) gr.style.display = 'none';
  }
}

function openSingleModal(modalId) {
  closeAllRegularModals();
  const el = document.getElementById(modalId);
  if (el) el.style.display = 'flex';
}

function isAnyModalVisible() {
  const modalIds = [
    'alert-modal',
    'modal-ant',
    'modal-java',
    'modal-db',
    'modal-settings',
    'modal-auto-config',
    'modal-add-group',
    'modal-group-run',
    'modal-confirm-clear-log',
    'modal-target-path-warning',
  ];
  return modalIds.some((id) => isModalVisible(id));
}

function normalizeProjectPathKeyLocal(rawPath) {
  let cleaned = String(rawPath || '').trim();
  if (
    (cleaned.startsWith('"') && cleaned.endsWith('"')) ||
    (cleaned.startsWith("'") && cleaned.endsWith("'"))
  ) {
    cleaned = cleaned.slice(1, -1).trim();
  }
  if (!cleaned) return '';

  cleaned = cleaned.replace(/\//g, '\\');
  const isUnc = cleaned.startsWith('\\\\');
  const body = isUnc ? cleaned.slice(2) : cleaned;
  const rawParts = body.split('\\');
  const resolved = [];

  for (let i = 0; i < rawParts.length; i++) {
    const seg = rawParts[i];
    if (!seg || seg === '.') continue;
    if (seg === '..') {
      const minKeep = !isUnc && resolved.length > 0 && /^[A-Za-z]:$/.test(resolved[0]) ? 1 : 0;
      if (resolved.length > minKeep) {
        resolved.pop();
      }
      continue;
    }
    resolved.push(seg);
  }

  let joined = (isUnc ? '\\\\' : '') + resolved.join('\\');
  if (/^[A-Za-z]:$/.test(joined)) {
    joined += '\\';
  }
  return joined.toLowerCase();
}

async function resolveNormalizedProjectPath(rawPath) {
  const trimmed = String(rawPath || '').trim().replace(/^["']+|["']+$/g, '');
  if (!trimmed) {
    return { display_path: '', key: '' };
  }
  try {
    const res = await invoke('normalize_project_path', { path: trimmed });
    if (res && typeof res === 'object' && res.key) {
      return res;
    }
  } catch (err) {
    console.warn('Fallback to local project path normalization:', err);
  }
  return {
    display_path: trimmed,
    key: normalizeProjectPathKeyLocal(trimmed),
  };
}

function createTabState(projectPath = '') {
  tabCounter += 1;
  return {
    id: `tab_${Date.now()}_${tabCounter}`,
    requestSeq: 0,
    projectPath: projectPath || '',
    projectPathKey: normalizeProjectPathKeyLocal(projectPath),
    buildDir: '',
    defaultTargetPath: '',
    customTargetPath: null,
    effectiveTargetPath: '',
    targetPathValidation: null,
    targets: [],
    defaultTarget: '',
    selectedTarget: '',
    propertiesFile: null,
    dbInfo: null,
    groups: [],
    activeGroupId: null,
    buildStatus: 'READY',
    buildStatusClass: 'badge-gray',
    footerStatusText: 'Ready',
    footerStatusTime: '',
    footerStatusKind: 'neutral',
    isBuilding: false,
    isGroupRunning: false,
    cmdPanelOpen: false,
    cmdPanelExpanded: false,
    consoleLogs: '',
    batFolder: '',
    batFilter: '',
    batFiles: [],
    batExecStatus: {},
    batExecMeta: {},
    selectedBatPath: '',
    selectedBatName: '',
    selectedBatContent: '',
    selectedBatPaths: [],
    lastSelectedBatIndex: -1,
    batchLogName: null,
    appLogName: null,
    logFile: null,
    logFolder: null,
  };
}

function resetTabProjectState(tab) {
  if (!tab) return;
  tab.buildDir = '';
  tab.defaultTargetPath = '';
  tab.customTargetPath = null;
  tab.effectiveTargetPath = '';
  tab.targetPathValidation = null;
  tab.targets = [];
  tab.defaultTarget = '';
  tab.selectedTarget = '';
  tab.propertiesFile = null;
  tab.dbInfo = null;
  tab.groups = [];
  tab.activeGroupId = null;
  tab.batFolder = '';
  tab.batFiles = [];
  tab.batExecStatus = {};
  tab.batExecMeta = {};
  tab.selectedBatPath = '';
  tab.selectedBatName = '';
  tab.selectedBatContent = '';
  tab.selectedBatPaths = [];
  tab.lastSelectedBatIndex = -1;
  tab.batchLogName = null;
  tab.appLogName = null;
  tab.logFile = null;
  tab.logFolder = null;
}

function getActiveTab() {
  return projectTabs.find((t) => t.id === activeTabId) || null;
}

function getTabById(tabId) {
  return projectTabs.find((t) => t.id === tabId) || null;
}

function getTabProjectKey(tab) {
  if (!tab) return '';
  return tab.projectPathKey || normalizeProjectPathKeyLocal(tab.projectPath);
}

async function openOrSwitchProject(rawPath) {
  const trimmed = String(rawPath || '').trim().replace(/^["']+|["']+$/g, '');
  if (!trimmed) {
    showAlert('Notice', 'Please select or enter a project directory path.', '[!]');
    return null;
  }

  const norm = await resolveNormalizedProjectPath(trimmed);
  const displayPath = norm.display_path || trimmed;
  const targetKey = norm.key || normalizeProjectPathKeyLocal(displayPath);

  let existingTab = projectTabs.find((t) => getTabProjectKey(t) === targetKey);
  if (!existingTab && targetKey) {
    for (const t of projectTabs) {
      if (t.projectPath && !t.projectPathKey) {
        const resolvedExisting = await resolveNormalizedProjectPath(t.projectPath);
        if (resolvedExisting.key) {
          t.projectPathKey = resolvedExisting.key;
        }
        if (getTabProjectKey(t) === targetKey) {
          existingTab = t;
          break;
        }
      }
    }
  }

  if (existingTab) {
    if (existingTab.id !== activeTabId) {
      switchToTab(existingTab.id);
    } else {
      const inputEl = document.getElementById('project-path-input');
      if (inputEl) {
        inputEl.value = existingTab.projectPath || displayPath;
        inputEl.dataset.tabId = existingTab.id;
      }
    }
    return existingTab;
  }

  const activeTab = getActiveTab();
  const canReuseBlankActive = activeTab && !(activeTab.projectPath || '').trim();
  const targetTab = canReuseBlankActive ? activeTab : addNewProjectTab(displayPath);
  targetTab.projectPath = displayPath;
  targetTab.projectPathKey = targetKey;

  if (targetTab.id === activeTabId) {
    const inputEl = document.getElementById('project-path-input');
    if (inputEl) {
      inputEl.value = displayPath;
      inputEl.dataset.tabId = targetTab.id;
    }
  }

  const newSettings = await invoke('add_project_history', { path: displayPath });
  historyListCache = newSettings.history || [];
  if (targetTab.id === activeTabId) {
    updateHistoryDropdown(historyListCache, displayPath);
  }
  await scanProjectForTab(targetTab, displayPath);
  await persistTabsToHistory();
  return targetTab;
}

function getTabShortTitle(projectPath) {
  if (!projectPath || projectPath.trim() === '') return 'New Project';
  const parts = projectPath.trim().split(/[\\/]/).filter(Boolean);
  return parts.length > 0 ? parts[parts.length - 1] : projectPath;
}

// Render the top Win95 Tab Strip
function renderTabsBar() {
  const listEl = document.getElementById('project-tabs-list');
  if (!listEl) return;

  listEl.innerHTML = '';
  projectTabs.forEach((tab, index) => {
    const tabEl = document.createElement('div');
    tabEl.className = `project-tab${tab.id === activeTabId ? ' active' : ''}`;
    tabEl.title = tab.projectPath || 'New Project Tab';

    const titleSpan = document.createElement('span');
    titleSpan.className = 'project-tab-title';
    const statusPrefix = tab.isBuilding ? '[...] ' : '';
    titleSpan.textContent = `${index + 1}. ${statusPrefix}${getTabShortTitle(tab.projectPath)}`;

    const closeBtn = document.createElement('span');
    closeBtn.className = 'project-tab-close';
    closeBtn.textContent = 'x';
    closeBtn.title = 'Close tab';

    closeBtn.addEventListener('click', (e) => {
      e.stopPropagation();
      closeProjectTab(tab.id);
    });

    tabEl.addEventListener('click', () => {
      if (tab.id !== activeTabId) {
        switchToTab(tab.id);
      }
    });

    tabEl.appendChild(titleSpan);
    tabEl.appendChild(closeBtn);
    listEl.appendChild(tabEl);
  });
}

async function persistTabsToHistory() {
  try {
    const openPaths = projectTabs.map((t) => t.projectPath || '');
    const activeIdx = Math.max(
      0,
      projectTabs.findIndex((t) => t.id === activeTabId)
    );
    await invoke('save_tabs_state', {
      openTabs: openPaths,
      activeTabIndex: activeIdx,
    });
  } catch (err) {
    console.error('Failed to persist tabs state:', err);
  }
}

function switchToTab(tabId) {
  const targetTab = getTabById(tabId);
  if (!targetTab) return;

  activeTabId = targetTab.id;
  renderTabsBar();
  syncUIToActiveTab();
  persistTabsToHistory();
}

function closeProjectTab(tabId) {
  const idx = projectTabs.findIndex((t) => t.id === tabId);
  if (idx === -1) return;

  projectTabs.splice(idx, 1);

  if (projectTabs.length === 0) {
    const freshTab = createTabState('');
    projectTabs.push(freshTab);
    activeTabId = freshTab.id;
  } else if (activeTabId === tabId) {
    const nextIdx = Math.min(idx, projectTabs.length - 1);
    activeTabId = projectTabs[nextIdx].id;
  }

  renderTabsBar();
  syncUIToActiveTab();
  persistTabsToHistory();
}

function addNewProjectTab(projectPath = '') {
  const newTab = createTabState(projectPath);
  projectTabs.push(newTab);
  activeTabId = newTab.id;
  renderTabsBar();
  syncUIToActiveTab();
  persistTabsToHistory();
  return newTab;
}

function formatCurrentTimeHHMMSS() {
  const now = new Date();
  const hh = String(now.getHours()).padStart(2, '0');
  const mm = String(now.getMinutes()).padStart(2, '0');
  const ss = String(now.getSeconds()).padStart(2, '0');
  return `${hh}:${mm}:${ss}`;
}

function formatCurrentDateTime() {
  const now = new Date();
  const yyyy = now.getFullYear();
  const mo = String(now.getMonth() + 1).padStart(2, '0');
  const dd = String(now.getDate()).padStart(2, '0');
  return `${yyyy}-${mo}-${dd} ${formatCurrentTimeHHMMSS()}`;
}

function getBatExecStatus(tab, batPath) {
  if (!tab || !tab.batExecStatus || !batPath) return '';
  return tab.batExecStatus[String(batPath).toLowerCase()] || '';
}

function setBatExecStatus(tab, batPath, status, meta = null) {
  if (!tab || !batPath) return;
  if (!tab.batExecStatus) tab.batExecStatus = {};
  if (!tab.batExecMeta) tab.batExecMeta = {};
  const key = String(batPath).toLowerCase();
  if (status) {
    tab.batExecStatus[key] = status;
  } else {
    delete tab.batExecStatus[key];
  }
  if (meta) {
    tab.batExecMeta[key] = meta;
  }
}

function renderFooterStatus(tab = getActiveTab()) {
  const footerStatus = document.getElementById('footer-status-text');
  if (!footerStatus) return;

  const text = (tab && tab.footerStatusText) || 'Ready';
  const timeStr = (tab && tab.footerStatusTime) || '';
  const kind = (tab && tab.footerStatusKind) || 'neutral';

  footerStatus.innerHTML = '';

  const labelSpan = document.createElement('span');
  labelSpan.className = `footer-status-label${kind === 'ok' ? ' status-ok' : kind === 'err' ? ' status-err' : ''}`;
  labelSpan.textContent = text;
  footerStatus.appendChild(labelSpan);

  if (timeStr) {
    const timeSpan = document.createElement('span');
    timeSpan.className = 'footer-status-time';
    timeSpan.textContent = timeStr;
    footerStatus.appendChild(timeSpan);
  }

  footerStatus.title = timeStr ? `${text} - ${timeStr}` : text;
}

function setTabFooterStatus(tab, text, timeStr = '', kind = 'neutral') {
  if (!tab) return;
  tab.footerStatusText = text || 'Ready';
  tab.footerStatusTime = timeStr || '';
  tab.footerStatusKind = kind || 'neutral';
  if (tab.id === activeTabId) {
    renderFooterStatus(tab);
  }
}

function updateFooterDbUI(tab = getActiveTab()) {
  const dbValEl = document.getElementById('footer-db-val');
  const dbBtnEl = document.getElementById('footer-db-btn');
  if (!dbValEl) return;

  const info = tab && tab.dbInfo ? tab.dbInfo : null;
  if (!info || !info.found) {
    dbValEl.textContent = 'N/A';
    dbValEl.style.color = '#666666';
    if (dbBtnEl) {
      dbBtnEl.title = 'Click to inspect & test Database configuration from selected target .properties';
    }
    return;
  }

  dbValEl.textContent = info.summary_label || 'Configured';
  dbValEl.style.color = info.warning_message ? '#995500' : '#000000';
  if (dbBtnEl) {
    const lines = [
      `DB Type: ${info.db_type || 'Unknown'}`,
      `Server: ${info.effective_server || info.host || 'N/A'}`,
      `Database: ${info.database || 'N/A'}`,
      `User: ${info.user || 'N/A'}`,
      `Config: ${info.properties_file || 'N/A'}`,
    ];
    dbBtnEl.title = lines.join('\n');
  }
}

// Sync all main workspace controls to the currently active tab
function syncUIToActiveTab() {
  const tab = getActiveTab();
  if (!tab) return;

  // 1. Project Path & History Dropdown
  const inputPath = document.getElementById('project-path-input');
  if (inputPath) {
    inputPath.value = tab.projectPath || '';
    inputPath.dataset.tabId = tab.id;
  }
  updateHistoryDropdown(historyListCache, tab.projectPath);
  renderTargetPathUI(tab);

  // 2. Target Dropdown, Show Config, Auto Config & Build Button
  const targetSelect = document.getElementById('select-ant-target');
  const showConfigBtn = document.getElementById('btn-show-config');
  const buildBtn = document.getElementById('btn-run-build');
  const hasTargets = !!(tab.projectPath && tab.targets && tab.targets.length > 0 && tab.selectedTarget);

  if (targetSelect) {
    targetSelect.innerHTML = '';
    if (!tab.projectPath) {
      const opt = document.createElement('option');
      opt.value = '';
      opt.textContent = '-- Select a project to load targets --';
      targetSelect.appendChild(opt);
    } else if (!tab.targets || tab.targets.length === 0) {
      const opt = document.createElement('option');
      opt.value = '';
      opt.textContent = '(No targets found)';
      targetSelect.appendChild(opt);
    } else {
      tab.targets.forEach((tgt) => {
        const opt = document.createElement('option');
        opt.value = tgt;
        opt.textContent = tgt;
        if (tgt === tab.selectedTarget) {
          opt.selected = true;
        }
        targetSelect.appendChild(opt);
      });
    }
  }
  if (showConfigBtn) {
    showConfigBtn.disabled = !hasTargets;
  }
  if (buildBtn) {
    buildBtn.disabled = !hasTargets || tab.isBuilding;
  }

  // 3. Build Status Badge & CMD Drawer State
  const badgeStatus = document.getElementById('badge-build-status');
  if (badgeStatus) {
    badgeStatus.textContent = tab.buildStatus || 'READY';
    badgeStatus.className = `badge ${tab.buildStatusClass || 'badge-gray'}`;
  }

  const openBuildBtn = document.getElementById('btn-open-build-dir');
  if (openBuildBtn) {
    openBuildBtn.disabled = !tab.buildDir;
  }

  const cmdPanel = document.getElementById('cmd-panel-box');
  const toggleCmdBtn = document.getElementById('btn-toggle-cmd');
  const expandCmdBtn = document.getElementById('btn-expand-cmd');

  if (cmdPanel) {
    cmdPanel.style.display = tab.cmdPanelOpen ? 'flex' : 'none';
    cmdPanel.classList.toggle('expanded', !!tab.cmdPanelExpanded);
  }
  if (toggleCmdBtn) {
    toggleCmdBtn.textContent = tab.cmdPanelOpen ? 'Hide CMD' : 'Show CMD';
  }
  if (expandCmdBtn) {
    expandCmdBtn.textContent = tab.cmdPanelExpanded ? 'Restore' : 'Expand';
  }

  // 4. Console Output
  const consoleEl = document.getElementById('build-console-output');
  if (consoleEl) {
    consoleEl.textContent = tab.consoleLogs || '';
    const wrapper = consoleEl.parentElement;
    if (wrapper) {
      wrapper.scrollTop = wrapper.scrollHeight;
    }
  }

  // 5. Batch Folder & Filter & Table
  const inputBatFolder = document.getElementById('input-bat-folder');
  const btnOpenBat = document.getElementById('btn-open-bat-dir');
  const inputBatFilter = document.getElementById('input-bat-filter');

  if (inputBatFolder) {
    inputBatFolder.value = tab.batFolder || '';
    inputBatFolder.title = tab.batFolder || '';
    inputBatFolder.dataset.tabId = tab.id;
  }
  if (btnOpenBat) {
    btnOpenBat.disabled = !tab.batFolder;
  }
  if (inputBatFilter) {
    inputBatFilter.value = tab.batFilter || '';
  }

  const clearAllBtn = document.getElementById('btn-clear-all-log');
  if (clearAllBtn) {
    clearAllBtn.disabled = !(
      (tab.projectPath && tab.projectPath.trim() !== '') ||
      (tab.consoleLogs && tab.consoleLogs.length > 0)
    );
  }

  renderFooterStatus(tab);

  updateFooterDbUI(tab);
  updateLogButtonsUI(tab);
  renderGroupsBar(tab);
  applyBatSplitWidth(batSplitLeftPct);
  applyBatThemeUI();
  applyBatFilterAndRender(tab);
  renderBatPreview(tab);
}

// Win95 Alert Dialog
function showAlert(title, message, icon = '[i]') {
  const modal = document.getElementById('alert-modal');
  const titleEl = document.getElementById('alert-title');
  const msgEl = document.getElementById('alert-message');
  const iconEl = document.getElementById('alert-icon');

  if (!modal) return;
  titleEl.textContent = title;
  msgEl.innerHTML = message;
  iconEl.textContent = icon;
  modal.style.display = 'flex';
}

function closeAlert() {
  const modal = document.getElementById('alert-modal');
  if (modal) modal.style.display = 'none';
}

// Append log line to a specific tab's CMD console
function appendBuildLogForTab(tabId, line) {
  const tab = (tabId && getTabById(tabId)) || getActiveTab();
  if (!tab) return;

  tab.consoleLogs += line + '\n';

  if (tab.id === activeTabId) {
    const consoleEl = document.getElementById('build-console-output');
    if (consoleEl) {
      consoleEl.textContent += line + '\n';
      const wrapper = consoleEl.parentElement;
      if (wrapper) {
        wrapper.scrollTop = wrapper.scrollHeight;
      }
    }
  }
}

// ==========================================================================
// 1. SETTINGS, HISTORY & TABS RESTORATION
// ==========================================================================
async function loadSettings() {
  try {
    const settingsPath = await invoke('get_settings_path');
    const footerSettings = document.getElementById('footer-settings-val');
    if (footerSettings) {
      footerSettings.textContent = `History: ${settingsPath}`;
      footerSettings.title = `History file: ${settingsPath}`;
    }

    const settings = await invoke('get_settings');
    historyListCache = settings.history || [];

    // Restore saved tabs (deduplicated by canonical normalized project path) or fallback to last_project
    const rawSavedTabs = Array.isArray(settings.open_tabs) ? settings.open_tabs : [];
    const targetRawActive = rawSavedTabs[settings.active_tab_index || 0] || '';
    const nonEmptySaved = rawSavedTabs.filter((p) => p && p.trim() !== '');

    if (nonEmptySaved.length > 0) {
      const normalizedList = await Promise.all(
        nonEmptySaved.map((p) => resolveNormalizedProjectPath(p))
      );
      const seenKeys = new Set();
      normalizedList.forEach((norm) => {
        const key = norm.key || normalizeProjectPathKeyLocal(norm.display_path);
        if (key && !seenKeys.has(key)) {
          seenKeys.add(key);
          const tab = createTabState(norm.display_path);
          tab.projectPathKey = key;
          projectTabs.push(tab);
        }
      });

      const activeNorm = targetRawActive
        ? await resolveNormalizedProjectPath(targetRawActive)
        : null;
      const matchedActive =
        activeNorm && activeNorm.key
          ? projectTabs.find((t) => getTabProjectKey(t) === activeNorm.key)
          : null;
      activeTabId = (matchedActive || projectTabs[0]).id;

      if (projectTabs.length < nonEmptySaved.length) {
        persistTabsToHistory();
      }
    } else if (settings.last_project && settings.last_project.trim() !== '') {
      const lastNorm = await resolveNormalizedProjectPath(settings.last_project.trim());
      const tab = createTabState(lastNorm.display_path || settings.last_project.trim());
      tab.projectPathKey = lastNorm.key || normalizeProjectPathKeyLocal(tab.projectPath);
      projectTabs.push(tab);
      activeTabId = tab.id;
    } else {
      const blankTab = createTabState('');
      projectTabs.push(blankTab);
      activeTabId = blankTab.id;
    }

    renderTabsBar();
    syncUIToActiveTab();

    // Scan active tab first so the visible workspace settles cleanly, then scan remaining restored tabs
    const orderedTabs = [];
    const activeTab = getActiveTab();
    if (activeTab) orderedTabs.push(activeTab);
    projectTabs.forEach((t) => {
      if (!activeTab || t.id !== activeTab.id) {
        orderedTabs.push(t);
      }
    });

    for (const tab of orderedTabs) {
      if (tab.projectPath && tab.projectPath.trim() !== '') {
        await scanProjectForTab(tab, tab.projectPath);
      }
    }
  } catch (err) {
    console.error('Error loading settings:', err);
  }
}

function closeHistoryMenu() {
  const menu = document.getElementById('history-dropdown-menu');
  if (menu) menu.style.display = 'none';
}

function updateHistoryDropdown(historyList, selectedProject) {
  const triggerBtn = document.getElementById('btn-history-dropdown');
  const labelEl = document.getElementById('history-dropdown-label');
  const menuEl = document.getElementById('history-dropdown-menu');
  if (!labelEl || !menuEl) return;

  const list = historyList || [];
  if (selectedProject && selectedProject.trim() !== '') {
    const cleanSelected = selectedProject.trim();
    const parts = cleanSelected.split(/[\\/]/).filter(Boolean);
    const shortName = parts.length > 0 ? parts[parts.length - 1] : cleanSelected;
    labelEl.textContent = `${shortName} (${cleanSelected})`;
    labelEl.title = cleanSelected;
    labelEl.dataset.fullPath = cleanSelected;
    if (triggerBtn) {
      triggerBtn.title = cleanSelected;
      triggerBtn.dataset.fullPath = cleanSelected;
    }
  } else if (list.length === 0) {
    labelEl.textContent = '(No history)';
    labelEl.title = 'No recent projects';
    labelEl.dataset.fullPath = '';
    if (triggerBtn) {
      triggerBtn.title = 'Select or remove recent projects';
      triggerBtn.dataset.fullPath = '';
    }
  } else {
    labelEl.textContent = '-- Select recent project --';
    labelEl.title = 'Select or remove recent projects';
    labelEl.dataset.fullPath = '';
    if (triggerBtn) {
      triggerBtn.title = 'Select or remove recent projects';
      triggerBtn.dataset.fullPath = '';
    }
  }

  menuEl.innerHTML = '';
  if (list.length === 0) {
    const emptyDiv = document.createElement('div');
    emptyDiv.className = 'history-empty-row';
    emptyDiv.textContent = '(No history)';
    menuEl.appendChild(emptyDiv);
    return;
  }

  const selectedKey = normalizeProjectPathKeyLocal(selectedProject);
  list.forEach((item) => {
    const row = document.createElement('div');
    const isActive = selectedKey !== '' && normalizeProjectPathKeyLocal(item) === selectedKey;
    row.className = `history-item-row${isActive ? ' active' : ''}`;
    row.title = item;
    row.dataset.fullPath = item;

    const textSpan = document.createElement('span');
    textSpan.className = 'history-item-text';
    const parts = item.split(/[\\/]/).filter(Boolean);
    const shortName = parts.length > 0 ? parts[parts.length - 1] : item;
    textSpan.textContent = `${shortName} (${item})`;
    textSpan.title = item;
    textSpan.dataset.fullPath = item;

    const delBtn = document.createElement('button');
    delBtn.type = 'button';
    delBtn.className = 'win95-btn history-item-delete';
    delBtn.textContent = 'x';
    delBtn.title = `Remove ${item} from history`;

    delBtn.addEventListener('click', async (e) => {
      e.stopPropagation();
      try {
        const updated = await invoke('remove_project_history', { path: item });
        historyListCache = updated.history || [];
        const activeTab = getActiveTab();
        updateHistoryDropdown(historyListCache, activeTab ? activeTab.projectPath : '');
      } catch (err) {
        console.error('Remove history item failed:', err);
        showAlert('History Error', `Failed to remove history item: ${err}`, '[X]');
      }
    });

    row.addEventListener('click', async () => {
      closeHistoryMenu();
      try {
        await openOrSwitchProject(item);
      } catch (err) {
        console.error('Open project from history failed:', err);
        showAlert('Open Project Error', escapeHtml(String(err)), '[X]');
      }
    });

    row.appendChild(textSpan);
    row.appendChild(delBtn);
    menuEl.appendChild(row);
  });
}

// ==========================================================================
// 2. PROJECT SCANNING & TARGET SELECTION (PER TAB)
// ==========================================================================
async function scanProjectForTab(tab, path) {
  if (!tab) return;
  if (!path || path.trim() === '') {
    showAlert('Notice', 'Please select or enter a project directory path.', '[!]');
    return;
  }

  const cleanPath = path.trim();
  const reqSeq = (tab.requestSeq || 0) + 1;
  tab.requestSeq = reqSeq;

  const incomingKey = normalizeProjectPathKeyLocal(cleanPath);
  const existingKey = getTabProjectKey(tab);
  const isSwitchingProject = existingKey !== '' && incomingKey !== existingKey;
  if (isSwitchingProject) {
    resetTabProjectState(tab);
  }
  tab.projectPath = cleanPath;
  if (!tab.projectPathKey || isSwitchingProject) {
    tab.projectPathKey = incomingKey;
  }
  renderTabsBar();
  if (isSwitchingProject && tab.id === activeTabId) {
    syncUIToActiveTab();
  }

  setTabFooterStatus(tab, 'Scanning...');

  try {
    const res = await invoke('scan_project_dir', { path: cleanPath });
    if (tab.requestSeq !== reqSeq) return;

    if (!res.is_valid_directory) {
      resetTabProjectState(tab);
      setTabFooterStatus(tab, 'Error');
      if (tab.id === activeTabId) {
        syncUIToActiveTab();
        showAlert('Invalid Directory', res.error_message || 'Directory is not valid.', '[X]');
      }
      return;
    }

    if (res.build_folder_found && res.build_folder_path) {
      tab.buildDir = res.build_folder_path;
    } else {
      tab.buildDir = cleanPath;
    }

    if (!res.build_xml_found || res.ant_targets.length === 0) {
      const savedBuildDir = tab.buildDir;
      resetTabProjectState(tab);
      tab.buildDir = savedBuildDir;
      try {
        const groups = await invoke('get_project_groups', { projectPath: cleanPath });
        if (tab.requestSeq !== reqSeq) return;
        tab.groups = Array.isArray(groups) ? groups : [];
      } catch (_) {
        tab.groups = [];
      }
      setTabFooterStatus(tab, !res.build_xml_found ? 'No build.xml' : 'No targets');
      if (tab.id === activeTabId) {
        syncUIToActiveTab();
      }
      return;
    }

    tab.targets = res.ant_targets;
    tab.defaultTarget = res.default_target || res.ant_targets[0];

    let targetToSelect = tab.defaultTarget;
    res.ant_targets.forEach((tgt) => {
      if (res.default_target && tgt.toLowerCase() === res.default_target.toLowerCase()) {
        targetToSelect = tgt;
      }
    });
    tab.selectedTarget = targetToSelect;

    try {
      const savedCustomTarget = await invoke('get_project_target_path', { projectPath: cleanPath });
      if (tab.requestSeq !== reqSeq) return;
      tab.customTargetPath = savedCustomTarget || null;
    } catch (_) {
      tab.customTargetPath = null;
    }

    await Promise.all([
      inspectTargetBatFilesForTab(tab, cleanPath, tab.selectedTarget, reqSeq),
      refreshTargetPathValidationForTab(tab, reqSeq),
    ]);
    if (tab.requestSeq !== reqSeq) return;

    setTabFooterStatus(tab, 'Ready');
    if (tab.id === activeTabId) {
      syncUIToActiveTab();
    }
  } catch (err) {
    console.error('Scan project failed:', err);
    if (tab.requestSeq === reqSeq) {
      setTabFooterStatus(tab, 'Error');
      if (tab.id === activeTabId) {
        showAlert('Error', `Failed to scan project: ${err}`, '[X]');
      }
    }
  }
}

// ==========================================================================
// 3. INSPECT TARGET BAT FILES, SYNTAX HIGHLIGHT & MULTI-SELECT (PER TAB)
// ==========================================================================
function updateLogButtonsUI(tab = getActiveTab()) {
  const btnBatchLog = document.getElementById('btn-open-log');
  const btnAppLog = document.getElementById('btn-open-app-log');

  if (btnBatchLog) {
    const bName = tab && tab.batchLogName ? tab.batchLogName : null;
    btnBatchLog.textContent = bName || 'File Log (N/A)';
    btnBatchLog.disabled = !bName;
    btnBatchLog.title = bName
      ? `Open ${bName} (configured in log4j properties)`
      : 'No batch log filename found in log4j properties';
  }

  if (btnAppLog) {
    const aName = tab && tab.appLogName ? tab.appLogName : null;
    btnAppLog.textContent = aName || 'App Log (N/A)';
    btnAppLog.disabled = !aName;
    btnAppLog.title = aName
      ? `Open ${aName} (configured in log4j properties)`
      : 'No app log filename found in log4j properties';
  }
}

function pruneTabBatSelections(tab) {
  const validLower = new Set((tab.batFiles || []).map((f) => f.path.toLowerCase()));
  tab.selectedBatPaths = (tab.selectedBatPaths || []).filter((p) =>
    validLower.has(String(p).toLowerCase())
  );
  if (tab.selectedBatPath && !validLower.has(tab.selectedBatPath.toLowerCase())) {
    tab.selectedBatPath = '';
    tab.selectedBatName = '';
    tab.selectedBatContent = '';
  }
}

function renderTargetPathUI(tab = getActiveTab()) {
  const inputTargetDir = document.getElementById('target-build-path-input');
  const btnBrowseTarget = document.getElementById('btn-browse-target-path');
  const btnResetTarget = document.getElementById('btn-reset-target-path');
  const badgeStatus = document.getElementById('badge-target-path-status');
  const warnBar = document.getElementById('target-path-warning-bar');
  const warnText = document.getElementById('target-path-warning-text');

  const hasProject = !!(tab && tab.projectPath && tab.projectPath.trim() !== '');
  const isBuilding = !!(tab && tab.isBuilding);
  const val = tab ? tab.targetPathValidation : null;
  const effectiveDisplay = hasProject
    ? tab.effectiveTargetPath ||
      (val && val.effective_target_path) ||
      tab.defaultTargetPath ||
      `${tab.projectPath}\\target`
    : '';

  if (inputTargetDir) {
    inputTargetDir.value = effectiveDisplay;
    inputTargetDir.disabled = !hasProject || isBuilding;
    inputTargetDir.dataset.tabId = tab ? tab.id : '';
    if (hasProject) {
      const modeLabel = tab.customTargetPath ? 'Custom Target Dir' : 'Default Target Dir';
      const defPath = tab.defaultTargetPath || `${tab.projectPath}\\target`;
      inputTargetDir.title = `${modeLabel}: ${effectiveDisplay}\nDefault: ${defPath}`;
    } else {
      inputTargetDir.title = 'Select a project directory first';
    }
  }

  if (btnBrowseTarget) {
    btnBrowseTarget.disabled = !hasProject || isBuilding;
  }
  if (btnResetTarget) {
    btnResetTarget.disabled = !hasProject || !tab.customTargetPath || isBuilding;
  }

  if (badgeStatus) {
    if (!hasProject || !val) {
      badgeStatus.textContent = 'PATH: N/A';
      badgeStatus.className = 'badge badge-gray target-path-status-badge';
      badgeStatus.title = 'Pre-build Windows CMD/BAT path length validation';
    } else if (val.severity === 'error') {
      const isLenErr =
        val.estimated_cmd_line_len > val.cmd_limit ||
        val.estimated_longest_file_path_len >= val.max_path_limit;
      badgeStatus.textContent = isLenErr ? 'PATH: TOO LONG' : 'PATH: INVALID';
      badgeStatus.className = 'badge badge-red target-path-status-badge';
      badgeStatus.title = `${val.summary}\nClick for full diagnostics.`;
    } else if (val.severity === 'warning') {
      badgeStatus.textContent = 'PATH: WARN';
      badgeStatus.className = 'badge badge-yellow target-path-status-badge';
      badgeStatus.title = `${val.summary}\nClick for diagnostics.`;
    } else {
      badgeStatus.textContent = 'PATH: OK';
      badgeStatus.className = 'badge badge-green target-path-status-badge';
      const cmdNote =
        val.estimated_cmd_line_len > 0
          ? `Est. CMD line: ${val.estimated_cmd_line_len} / ${val.cmd_limit} chars`
          : 'CMD line: N/A';
      const safeCap =
        val.max_safe_target_len !== null && val.max_safe_target_len !== undefined
          ? ` | Max safe Target Dir: ${val.max_safe_target_len} chars`
          : '';
      badgeStatus.title = `${cmdNote} | Max output path: ${val.estimated_longest_file_path_len} / ${val.max_path_limit} chars${safeCap}`;
    }
  }

  if (warnBar && warnText) {
    if (hasProject && val && (val.severity === 'error' || val.severity === 'warning')) {
      warnBar.style.display = 'flex';
      warnBar.classList.toggle('is-warning', val.severity === 'warning');
      const firstMsg =
        (val.errors && val.errors[0]) ||
        (val.warnings && val.warnings[0]) ||
        val.summary ||
        '';
      warnText.textContent = firstMsg;
      warnBar.title = val.summary || firstMsg;
    } else {
      warnBar.style.display = 'none';
      warnText.textContent = '';
      warnBar.title = '';
    }
  }
}

async function refreshTargetPathValidationForTab(tab, expectedSeq = null) {
  if (!tab || !tab.projectPath) {
    if (tab) {
      tab.targetPathValidation = null;
      if (tab.id === activeTabId) renderTargetPathUI(tab);
    }
    return null;
  }

  const projectPath = tab.projectPath;
  const target = tab.selectedTarget || '';
  const customTargetPath = tab.customTargetPath || null;
  const reqSeq = expectedSeq !== null ? expectedSeq : tab.requestSeq || 0;
  const jh = selectedJavaHome || (envData && envData.java_home) || null;

  try {
    const validation = await invoke('validate_target_build_path', {
      projectPath,
      target,
      customTargetPath,
      javaHome: jh,
    });
    if (
      (tab.requestSeq || 0) !== reqSeq ||
      normalizeProjectPathKeyLocal(tab.projectPath) !== normalizeProjectPathKeyLocal(projectPath)
    ) {
      return null;
    }
    tab.defaultTargetPath = validation.default_target_path || '';
    tab.effectiveTargetPath = validation.effective_target_path || '';
    if (validation.is_default_target) {
      tab.customTargetPath = null;
    }
    tab.targetPathValidation = validation;
    if (tab.id === activeTabId) {
      renderTargetPathUI(tab);
    }
    return validation;
  } catch (err) {
    console.error('Validate target build path failed:', err);
    return null;
  }
}

async function applyTargetBuildPathForActiveTab(
  rawPath,
  { resetToDefault = false, explicitTab = null } = {}
) {
  const tab = explicitTab || getActiveTab();
  if (!tab || !tab.projectPath) return null;
  if (tab.isBuilding) {
    showAlert('Build in Progress', 'Cannot change Target Dir while Ant build is running.', '[!]');
    return null;
  }

  const jh = selectedJavaHome || (envData && envData.java_home) || null;
  const trimmed = String(rawPath || '').trim();
  const res = await invoke('set_project_target_path', {
    projectPath: tab.projectPath,
    target: tab.selectedTarget || null,
    customTargetPath: resetToDefault || !trimmed ? null : trimmed,
    javaHome: jh,
  });

  tab.customTargetPath = res.is_default_target ? null : res.custom_target_path || null;
  tab.defaultTargetPath = res.default_target_path || '';
  tab.effectiveTargetPath = res.effective_target_path || res.default_target_path || '';
  tab.targetPathValidation = res.validation || null;

  if (tab.selectedTarget) {
    await inspectTargetBatFilesForTab(tab, tab.projectPath, tab.selectedTarget);
  }
  if (tab.id === activeTabId) {
    renderTargetPathUI(tab);
  }

  if (res.validation && res.validation.is_valid) {
    const modeLabel = res.is_default_target ? 'Default' : 'Custom';
    setTabFooterStatus(
      tab,
      `Target Dir set (${modeLabel})`,
      formatCurrentDateTime(),
      'ok'
    );
  } else if (res.validation) {
    setTabFooterStatus(tab, 'Target Dir issue', formatCurrentDateTime(), 'err');
  }

  return res;
}

function populateTargetPathWarningModal(validation) {
  if (!validation) return;
  const summaryEl = document.getElementById('tpw-summary');
  const curPathEl = document.getElementById('tpw-current-path');
  const cmdEl = document.getElementById('tpw-cmd-usage');
  const maxPathEl = document.getElementById('tpw-max-path-usage');
  const safeLimitEl = document.getElementById('tpw-safe-limit');
  const offendingEl = document.getElementById('tpw-offending-script');
  const detailsEl = document.getElementById('tpw-details-list');

  if (summaryEl) {
    summaryEl.textContent = validation.is_valid
      ? 'Target Build Path validation passed (close to limit warning).'
      : 'Build cannot start because the current Target Build Path will fail or exceed Windows path/CMD limits. Select a shorter Target Dir (e.g. C:\\bh_target\\batch) before building.';
  }
  if (curPathEl) {
    curPathEl.textContent = `${validation.effective_target_path} (${validation.effective_target_len} chars; batch-safe: ${validation.batch_safe_target_len} chars)`;
    curPathEl.title = validation.effective_target_path || '';
  }
  if (cmdEl) {
    if (validation.estimated_cmd_line_len > 0) {
      const state =
        validation.estimated_cmd_line_len > validation.cmd_limit ? 'EXCEEDS LIMIT' : 'OK';
      cmdEl.textContent = `${validation.estimated_cmd_line_len} / ${validation.cmd_limit} chars (${validation.jar_reference_count} %BATCH_HOME% JAR refs) [${state}]`;
      cmdEl.style.color =
        validation.estimated_cmd_line_len > validation.cmd_limit ? '#bb0000' : '#006600';
    } else {
      cmdEl.textContent = `N/A (no SetBatchPath.bat found; limit ${validation.cmd_limit} chars)`;
      cmdEl.style.color = '#333333';
    }
  }
  if (maxPathEl) {
    const pathOver = validation.estimated_longest_file_path_len >= validation.max_path_limit;
    maxPathEl.textContent = `${validation.estimated_longest_file_path_len} / ${validation.max_path_limit} chars [${pathOver ? 'EXCEEDS MAX_PATH' : 'OK'}]`;
    maxPathEl.style.color = pathOver ? '#bb0000' : '#006600';
  }
  if (safeLimitEl) {
    safeLimitEl.textContent =
      validation.max_safe_target_len !== null && validation.max_safe_target_len !== undefined
        ? `<= ${validation.max_safe_target_len} chars (recommended short folder without spaces, e.g. C:\\bh_target\\job)`
        : '<= 80 chars recommended';
  }
  if (offendingEl) {
    if (validation.offending_script) {
      const lineStr = validation.offending_line_number
        ? ` line ${validation.offending_line_number}`
        : '';
      offendingEl.textContent = `${validation.offending_script}${lineStr}`;
    } else {
      offendingEl.textContent = 'Target path check';
    }
  }
  if (detailsEl) {
    const allMsgs = [...(validation.errors || []), ...(validation.warnings || [])];
    detailsEl.innerHTML =
      allMsgs.length > 0
        ? allMsgs.map((m) => `<div>- ${escapeHtml(m)}</div>`).join('')
        : '<div>No issues detected.</div>';
  }
}

function openTargetPathWarningModal(validation) {
  populateTargetPathWarningModal(validation);
  openSingleModal('modal-target-path-warning');
}

function closeTargetPathWarningModal() {
  const el = document.getElementById('modal-target-path-warning');
  if (el) el.style.display = 'none';
}

async function refreshDatabaseInfoForTab(tab, expectedSeq = null) {
  if (!tab || !tab.projectPath || !tab.selectedTarget) {
    if (tab) {
      tab.dbInfo = null;
      if (tab.id === activeTabId) updateFooterDbUI(tab);
    }
    return null;
  }

  const projectPath = tab.projectPath;
  const target = tab.selectedTarget;
  const reqSeq = expectedSeq !== null ? expectedSeq : tab.requestSeq || 0;

  try {
    const dbInfo = await invoke('inspect_database_config', { projectPath, target });
    if (
      (tab.requestSeq || 0) !== reqSeq ||
      (tab.projectPath || '').trim().toLowerCase() !== projectPath.trim().toLowerCase() ||
      tab.selectedTarget !== target
    ) {
      return null;
    }
    tab.dbInfo = dbInfo;
    if (tab.id === activeTabId) {
      updateFooterDbUI(tab);
    }
    return dbInfo;
  } catch (err) {
    console.error('Inspect database config failed:', err);
    return null;
  }
}

async function inspectTargetBatFilesForTab(tab, projectPath, target, expectedSeq = null) {
  if (!tab || !projectPath || !target) return;

  const reqSeq = expectedSeq !== null ? expectedSeq : (tab.requestSeq || 0) + 1;
  if (expectedSeq === null) {
    tab.requestSeq = reqSeq;
  }

  try {
    const [inspection, dbInfo, groups] = await Promise.all([
      invoke('inspect_build_target', {
        projectPath,
        target,
        customTargetPath: tab.customTargetPath || null,
      }),
      invoke('inspect_database_config', { projectPath, target }).catch(() => null),
      invoke('get_project_groups', { projectPath }).catch(() => tab.groups || []),
    ]);
    if (
      tab.requestSeq !== reqSeq ||
      (tab.projectPath || '').trim().toLowerCase() !== projectPath.trim().toLowerCase() ||
      tab.selectedTarget !== target
    ) {
      return;
    }

    tab.propertiesFile = inspection.properties_file || null;
    if (inspection.default_target_path) {
      tab.defaultTargetPath = inspection.default_target_path;
    }
    if (inspection.effective_target_path) {
      tab.effectiveTargetPath = inspection.effective_target_path;
    }
    tab.dbInfo = dbInfo;
    tab.groups = Array.isArray(groups) ? groups : [];
    if (tab.activeGroupId && !tab.groups.some((g) => g.id === tab.activeGroupId)) {
      tab.activeGroupId = null;
    }
    tab.batFolder = inspection.bat_folder || '';
    tab.batchLogName = inspection.batch_log_name || null;
    tab.appLogName = inspection.app_log_name || null;
    tab.logFile = inspection.log_file_path || null;
    tab.logFolder = inspection.log_folder_path || null;
    tab.batFiles = inspection.bat_files || [];

    pruneTabBatSelections(tab);

    if (tab.selectedBatPath) {
      const existing = tab.batFiles.find(
        (f) => f.path.toLowerCase() === tab.selectedBatPath.toLowerCase()
      );
      if (existing) {
        await loadBatFileContentForTab(tab, existing);
        if (tab.requestSeq !== reqSeq) return;
      }
    }

    if (tab.id === activeTabId && tab.requestSeq === reqSeq) {
      const inputBatFolder = document.getElementById('input-bat-folder');
      const btnOpenBat = document.getElementById('btn-open-bat-dir');
      const btnShowConfig = document.getElementById('btn-show-config');
      if (inputBatFolder) {
        inputBatFolder.value = tab.batFolder;
        inputBatFolder.dataset.tabId = tab.id;
        inputBatFolder.title = tab.batFolder
          ? `Base Dir: ${inspection.base_directory || 'N/A'}\nConfig: ${inspection.properties_file || 'N/A'}`
          : '';
      }
      if (btnOpenBat) {
        btnOpenBat.disabled = !tab.batFolder;
      }
      if (btnShowConfig) {
        btnShowConfig.disabled = !(tab.projectPath && tab.selectedTarget);
      }
      updateFooterDbUI(tab);
      updateLogButtonsUI(tab);
      renderGroupsBar(tab);
      applyBatFilterAndRender(tab);
      renderBatPreview(tab);
    }
  } catch (err) {
    console.error('Inspect bat files failed:', err);
  }
}

async function scanCustomBatFolderForActiveTab(folderPath, explicitTab = null) {
  const tab = explicitTab || getActiveTab();
  const cleanFolder = (folderPath || '').trim();
  if (!tab || !cleanFolder || cleanFolder === (tab.batFolder || '').trim()) return;

  const reqSeq = (tab.requestSeq || 0) + 1;
  tab.requestSeq = reqSeq;

  try {
    const [resolvedDir, files] = await invoke('scan_custom_bat_dir', {
      folderPath: cleanFolder,
    });
    if (tab.requestSeq !== reqSeq) return;

    tab.batFolder = resolvedDir || cleanFolder;
    tab.batFiles = files || [];

    pruneTabBatSelections(tab);

    if (tab.selectedBatPath) {
      const existing = tab.batFiles.find(
        (f) => f.path.toLowerCase() === tab.selectedBatPath.toLowerCase()
      );
      if (existing) {
        await loadBatFileContentForTab(tab, existing);
        if (tab.requestSeq !== reqSeq) return;
      }
    }

    if (tab.id === activeTabId && tab.requestSeq === reqSeq) {
      const inputBatFolder = document.getElementById('input-bat-folder');
      const btnOpenBat = document.getElementById('btn-open-bat-dir');
      if (inputBatFolder && resolvedDir) {
        inputBatFolder.value = resolvedDir;
        inputBatFolder.title = resolvedDir;
        inputBatFolder.dataset.tabId = tab.id;
      }
      if (btnOpenBat) {
        btnOpenBat.disabled = !tab.batFolder;
      }
      applyBatFilterAndRender(tab);
      renderBatPreview(tab);
    }
  } catch (err) {
    console.error('Custom bat folder scan failed:', err);
    if (tab.id === activeTabId) {
      showAlert('Error', `Failed to scan folder: ${err}`, '[X]');
    }
  }
}

function applyBatSplitWidth(pct) {
  const clamped = Math.max(20, Math.min(80, pct));
  batSplitLeftPct = clamped;
  const leftPane = document.getElementById('bat-list-pane');
  if (leftPane) {
    leftPane.style.flexBasis = `${clamped}%`;
  }
}

function applyBatThemeUI() {
  const tableContainer = document.getElementById('bat-table-container');
  const viewer = document.getElementById('bat-preview-content');
  const groupLogViewer = document.getElementById('group-run-output-log');
  const toggleBtn = document.getElementById('btn-toggle-bat-theme');
  if (tableContainer) {
    tableContainer.classList.toggle('theme-dark', !!batViewerDarkTheme);
  }
  if (viewer) {
    viewer.classList.toggle('theme-dark', !!batViewerDarkTheme);
  }
  if (groupLogViewer) {
    groupLogViewer.classList.toggle('theme-dark', !!batViewerDarkTheme);
  }
  if (toggleBtn) {
    toggleBtn.textContent = batViewerDarkTheme ? 'Light' : 'Dark';
    toggleBtn.title = batViewerDarkTheme
      ? 'Switch BAT viewer to Light theme'
      : 'Switch BAT viewer to Dark theme';
  }
}

// ==========================================================================
// BAT SYNTAX HIGHLIGHTER (Preserves Japanese & Unicode text cleanly)
// ==========================================================================
const BAT_COMMANDS = new Set([
  'echo',
  'set',
  'call',
  'cd',
  'chdir',
  'md',
  'mkdir',
  'rd',
  'rmdir',
  'del',
  'erase',
  'copy',
  'xcopy',
  'robocopy',
  'move',
  'ren',
  'rename',
  'type',
  'cls',
  'title',
  'color',
  'pause',
  'pushd',
  'popd',
  'setlocal',
  'endlocal',
  'shift',
  'start',
  'cmd',
  'powershell',
  'java',
  'javac',
  'ant',
  'exit',
  'dir',
  'find',
  'findstr',
  'timeout',
]);

const BAT_KEYWORDS = new Set([
  'if',
  'else',
  'for',
  'in',
  'do',
  'goto',
  'not',
  'exist',
  'defined',
  'equ',
  'neq',
  'lss',
  'leq',
  'gtr',
  'geq',
  'errorlevel',
  'on',
  'off',
]);

function highlightVarSegmentsInText(rawText, defaultClass = '') {
  const pattern = /(%[A-Za-z_][A-Za-z0-9_]*%|![A-Za-z_][A-Za-z0-9_]*!|%%[A-Za-z0-9*]|%[0-9*]|@[A-Za-z0-9_.-]+@)/g;
  let out = '';
  let lastIdx = 0;
  let match;

  while ((match = pattern.exec(rawText)) !== null) {
    if (match.index > lastIdx) {
      const plain = escapeHtml(rawText.slice(lastIdx, match.index));
      out += defaultClass ? `<span class="${defaultClass}">${plain}</span>` : plain;
    }
    const tok = match[0];
    let cls = 'bat-tok-var';
    if (tok.startsWith('@')) {
      cls = 'bat-tok-filter';
    } else if (/^%%|^%[0-9*]$/.test(tok)) {
      cls = 'bat-tok-var-arg';
    }
    out += `<span class="${cls}">${escapeHtml(tok)}</span>`;
    lastIdx = match.index + tok.length;
  }

  if (lastIdx < rawText.length) {
    const tail = escapeHtml(rawText.slice(lastIdx));
    out += defaultClass ? `<span class="${defaultClass}">${tail}</span>` : tail;
  }

  return out;
}

function highlightBatWordToken(token, ctx) {
  const lower = token.toLowerCase();

  // Command right after line start / @ / SET target
  if (ctx.expectLabel) {
    ctx.expectLabel = false;
    return `<span class="bat-tok-label">${escapeHtml(token)}</span>`;
  }

  if (ctx.expectSetVar) {
    ctx.expectSetVar = false;
    return `<span class="bat-tok-var">${escapeHtml(token)}</span>`;
  }

  if (ctx.atCommandPos && BAT_COMMANDS.has(lower)) {
    ctx.atCommandPos = false;
    if (lower === 'set') {
      ctx.afterSet = true;
    }
    return `<span class="bat-tok-cmd">${escapeHtml(token)}</span>`;
  }

  if (BAT_KEYWORDS.has(lower)) {
    if (lower === 'goto') {
      ctx.expectLabel = true;
    }
    if (lower === 'do' || lower === 'else') {
      ctx.atCommandPos = true;
    } else {
      ctx.atCommandPos = false;
    }
    return `<span class="bat-tok-kw">${escapeHtml(token)}</span>`;
  }

  ctx.atCommandPos = false;

  if (token.includes('%') || token.includes('!') || /@[A-Za-z0-9_.-]+@/.test(token)) {
    if (token.includes('\\') || token.includes('/') || /^[A-Za-z]:/.test(token)) {
      return highlightVarSegmentsInText(token, 'bat-tok-path');
    }
    return highlightVarSegmentsInText(token, '');
  }

  if (/^\/[A-Za-z0-9?:_-]+$/.test(token) || /^--?[A-Za-z0-9_.:=-]+$/.test(token)) {
    return `<span class="bat-tok-param">${escapeHtml(token)}</span>`;
  }

  if (
    token.includes('\\') ||
    token.includes('/') ||
    /^[A-Za-z]:/.test(token) ||
    /\.(bat|cmd|exe|jar|properties|xml|log|txt|sql|sh|csv)$/i.test(token)
  ) {
    return `<span class="bat-tok-path">${escapeHtml(token)}</span>`;
  }

  if (/^\d+$/.test(token)) {
    return `<span class="bat-tok-var-arg">${escapeHtml(token)}</span>`;
  }

  return escapeHtml(token);
}

function highlightBatLine(line) {
  const leadMatch = line.match(/^([ \t]*)(.*)$/);
  const indent = leadMatch ? leadMatch[1] : '';
  let rest = leadMatch ? leadMatch[2] : line;

  if (!rest) {
    return escapeHtml(indent);
  }

  // Comments: REM ... or :: ...
  if (/^@?rem(?:\s|$)/i.test(rest) || /^::/.test(rest)) {
    return `${escapeHtml(indent)}<span class="bat-tok-comment">${escapeHtml(rest)}</span>`;
  }

  // Label line: :labelName
  if (/^:[^\s:]+/.test(rest)) {
    return `${escapeHtml(indent)}<span class="bat-tok-label">${escapeHtml(rest)}</span>`;
  }

  let out = escapeHtml(indent);
  if (rest.startsWith('@')) {
    out += `<span class="bat-tok-op">@</span>`;
    rest = rest.slice(1);
  }

  const ctx = {
    atCommandPos: true,
    afterSet: false,
    expectSetVar: false,
    expectLabel: false,
  };

  let i = 0;
  const n = rest.length;

  while (i < n) {
    const ch = rest[i];

    // Whitespace
    if (ch === ' ' || ch === '\t') {
      let j = i;
      while (j < n && (rest[j] === ' ' || rest[j] === '\t')) j++;
      out += escapeHtml(rest.slice(i, j));
      i = j;
      continue;
    }

    // Quoted string
    if (ch === '"') {
      let j = i + 1;
      while (j < n && rest[j] !== '"') j++;
      if (j < n && rest[j] === '"') j++;
      const strTok = rest.slice(i, j);
      out += highlightVarSegmentsInText(strTok, 'bat-tok-string');
      ctx.atCommandPos = false;
      ctx.afterSet = false;
      i = j;
      continue;
    }

    // Two-char / single-char operators & punctuation
    const two = rest.slice(i, i + 2);
    if (two === '>>' || two === '&&' || two === '||') {
      out += `<span class="bat-tok-op">${escapeHtml(two)}</span>`;
      if (two === '&&' || two === '||') {
        ctx.atCommandPos = true;
      }
      ctx.afterSet = false;
      i += 2;
      continue;
    }

    if (ch === '>' || ch === '<' || ch === '|' || ch === '&' || ch === '=' || ch === '(' || ch === ')') {
      out += `<span class="bat-tok-op">${escapeHtml(ch)}</span>`;
      if (ch === '|' || ch === '&' || ch === '(') {
        ctx.atCommandPos = true;
      }
      ctx.afterSet = false;
      i += 1;
      continue;
    }

    // Word / path / variable / argument token until whitespace or operator
    let j = i;
    while (
      j < n &&
      rest[j] !== ' ' &&
      rest[j] !== '\t' &&
      rest[j] !== '"' &&
      rest[j] !== '>' &&
      rest[j] !== '<' &&
      rest[j] !== '|' &&
      rest[j] !== '&' &&
      rest[j] !== '(' &&
      rest[j] !== ')'
    ) {
      if (rest[j] === '=' && j > i) {
        break;
      }
      if (rest[j] === '=' && j === i) {
        break;
      }
      j++;
    }

    if (j === i) {
      out += escapeHtml(rest[i]);
      i += 1;
      continue;
    }

    const tok = rest.slice(i, j);
    if (ctx.afterSet && rest[j] === '=') {
      ctx.afterSet = false;
      out += `<span class="bat-tok-var">${escapeHtml(tok)}</span>`;
    } else {
      ctx.afterSet = false;
      out += highlightBatWordToken(tok, ctx);
    }
    i = j;
  }

  return out;
}

function highlightBatContent(content) {
  const lines = String(content || '').split(/\r?\n/);
  return lines.map((line) => highlightBatLine(line)).join('\n');
}

async function loadBatFileContentForTab(tab, file) {
  if (!tab || !file || !file.path) return;

  const targetPath = file.path;
  const reqSeq = tab.requestSeq || 0;
  tab.selectedBatPath = targetPath;
  tab.selectedBatName = file.name;
  tab.selectedBatContent = 'Loading file content...';

  if (tab.id === activeTabId) {
    applyBatFilterAndRender(tab);
    renderBatPreview(tab);
  }

  try {
    const content = await invoke('read_text_file', { path: targetPath });
    if ((tab.requestSeq || 0) === reqSeq && tab.selectedBatPath === targetPath) {
      tab.selectedBatContent = content;
    }
  } catch (err) {
    if ((tab.requestSeq || 0) === reqSeq && tab.selectedBatPath === targetPath) {
      tab.selectedBatContent = `[Error reading file] ${err}`;
    }
  }

  if (tab.id === activeTabId && (tab.requestSeq || 0) === reqSeq) {
    renderBatPreview(tab);
  }
}

function renderBatPreview(tab = getActiveTab()) {
  const titleEl = document.getElementById('bat-preview-title');
  const contentEl = document.getElementById('bat-preview-content');
  const runBtn = document.getElementById('btn-run-selected-bat');
  const openFileBtn = document.getElementById('btn-open-selected-bat');
  if (!titleEl || !contentEl) return;

  applyBatThemeUI();

  if (!tab || !tab.selectedBatPath) {
    titleEl.textContent = 'Select a .bat file to view content';
    titleEl.title = '';
    contentEl.textContent =
      'Select a .bat file from the left list to inspect its script content.';
    if (runBtn) runBtn.disabled = true;
    if (openFileBtn) openFileBtn.disabled = true;
    return;
  }

  titleEl.textContent = tab.selectedBatName || tab.selectedBatPath;
  titleEl.title = tab.selectedBatPath;
  if (!tab.selectedBatContent) {
    contentEl.textContent = '(Empty file)';
  } else {
    contentEl.innerHTML = highlightBatContent(tab.selectedBatContent);
  }
  if (runBtn) runBtn.disabled = false;
  if (openFileBtn) openFileBtn.disabled = false;
}

function filterBatFilesByPrefixes(files, rawQuery) {
  const list = files || [];
  const trimmed = String(rawQuery || '').trim();
  if (!trimmed) return list;

  const tokens = [];
  const tokenSet = new Set();
  trimmed.split(',').forEach((part) => {
    const clean = part.trim().toLowerCase();
    if (clean && !tokenSet.has(clean)) {
      tokenSet.add(clean);
      tokens.push(clean);
    }
  });

  if (tokens.length === 0) return list;

  const matched = [];
  const seenPaths = new Set();
  for (let i = 0; i < list.length; i++) {
    const file = list[i];
    const nameLower = String(file.name || '').toLowerCase();
    let isMatch = false;
    for (let t = 0; t < tokens.length; t++) {
      if (nameLower.startsWith(tokens[t])) {
        isMatch = true;
        break;
      }
    }
    if (isMatch) {
      const pathKey = String(file.path || file.name || i).toLowerCase();
      if (!seenPaths.has(pathKey)) {
        seenPaths.add(pathKey);
        matched.push(file);
      }
    }
  }

  return matched;
}

function getFileBasename(filePath) {
  const parts = String(filePath || '').trim().split(/[\\/]/).filter(Boolean);
  return parts.length > 0 ? parts[parts.length - 1] : String(filePath || '');
}

function resolveGroupOrderedFiles(tab, group) {
  if (!tab || !group || !Array.isArray(group.files)) return [];
  const allFiles = tab.batFiles || [];
  const byPath = new Map();
  const byName = new Map();
  allFiles.forEach((f) => {
    byPath.set(String(f.path || '').toLowerCase(), f);
    if (!byName.has(String(f.name || '').toLowerCase())) {
      byName.set(String(f.name || '').toLowerCase(), f);
    }
  });

  const resolved = [];
  const seen = new Set();
  group.files.forEach((savedPath) => {
    const cleanPath = String(savedPath || '').trim();
    if (!cleanPath) return;
    const key = cleanPath.toLowerCase();
    if (seen.has(key)) return;
    seen.add(key);

    const exact = byPath.get(key);
    if (exact) {
      resolved.push(exact);
      return;
    }

    const baseName = getFileBasename(cleanPath);
    const matchedByName = byName.get(baseName.toLowerCase());
    if (matchedByName) {
      resolved.push(matchedByName);
      return;
    }

    resolved.push({
      name: baseName,
      path: cleanPath,
      size_bytes: 0,
      modified_time: 'Missing',
      isMissing: true,
    });
  });

  return resolved;
}

function getBaseBatFilesForTab(tab = getActiveTab()) {
  if (!tab) return [];
  if (!tab.activeGroupId) {
    return tab.batFiles || [];
  }
  const activeGroup = (tab.groups || []).find((g) => g.id === tab.activeGroupId);
  if (!activeGroup) {
    return tab.batFiles || [];
  }
  return resolveGroupOrderedFiles(tab, activeGroup);
}

function getFilteredBatFiles(tab = getActiveTab()) {
  if (!tab) return [];
  const filterInput = document.getElementById('input-bat-filter');
  const rawQuery =
    tab.id === activeTabId && filterInput
      ? filterInput.value
      : tab.batFilter || '';
  const baseFiles = getBaseBatFilesForTab(tab);
  return filterBatFilesByPrefixes(baseFiles, rawQuery);
}

function getSelectedFilesInDisplayOrder(tab = getActiveTab()) {
  if (!tab) return [];
  const selectedSet = new Set((tab.selectedBatPaths || []).map((p) => p.toLowerCase()));
  if (selectedSet.size === 0 && tab.selectedBatPath) {
    selectedSet.add(tab.selectedBatPath.toLowerCase());
  }
  const filtered = getFilteredBatFiles(tab);
  return filtered.filter((f) => f && !f.isMissing && selectedSet.has(f.path.toLowerCase()));
}

function updateSelectedCountBadge(tab = getActiveTab()) {
  const badge = document.getElementById('meta-bat-selected');
  if (!badge) return;
  const count = tab ? getSelectedFilesInDisplayOrder(tab).length : 0;
  if (count > 1) {
    badge.textContent = `${count} sel`;
    badge.style.display = 'inline-block';
  } else {
    badge.style.display = 'none';
  }
}

function closeBatContextMenu() {
  const menu = document.getElementById('bat-context-menu');
  if (menu) menu.style.display = 'none';
}

function closeGroupContextMenu() {
  const menu = document.getElementById('group-context-menu');
  if (menu) menu.style.display = 'none';
}

function positionPopupMenu(menu, x, y) {
  menu.style.display = 'block';
  const menuW = menu.offsetWidth || 180;
  const menuH = menu.offsetHeight || 40;
  const clampedX = Math.max(4, Math.min(x, window.innerWidth - menuW - 6));
  const clampedY = Math.max(4, Math.min(y, window.innerHeight - menuH - 6));
  menu.style.left = `${clampedX}px`;
  menu.style.top = `${clampedY}px`;
}

function renderGroupsBar(tab = getActiveTab()) {
  const barEl = document.getElementById('bat-groups-bar');
  const listEl = document.getElementById('bat-groups-list');
  if (!barEl || !listEl) return;

  const groups = (tab && Array.isArray(tab.groups) && tab.groups) || [];
  if (groups.length === 0) {
    barEl.style.display = 'none';
    listEl.innerHTML = '';
    return;
  }

  barEl.style.display = 'flex';
  listEl.innerHTML = '';

  const totalCount = (tab.batFiles || []).length;
  const allChip = document.createElement('button');
  allChip.type = 'button';
  allChip.className = `win95-btn group-chip${!tab.activeGroupId ? ' active' : ''}`;
  allChip.title = 'Return to normal full BAT file list';
  allChip.innerHTML = `<span class="group-chip-name">All</span><span class="group-chip-count">(${totalCount})</span>`;
  allChip.addEventListener('click', () => {
    closeBatContextMenu();
    closeGroupContextMenu();
    tab.activeGroupId = null;
    renderGroupsBar(tab);
    applyBatFilterAndRender(tab);
  });
  listEl.appendChild(allChip);

  groups.forEach((group) => {
    const isActive = tab.activeGroupId === group.id;
    const chip = document.createElement('div');
    chip.setAttribute('role', 'button');
    chip.tabIndex = 0;
    chip.className = `win95-btn group-chip${isActive ? ' active' : ''}`;
    chip.title = `${group.name} (${group.files.length} file(s)) | Click to filter | Right-click for Run All`;

    const nameSpan = document.createElement('span');
    nameSpan.className = 'group-chip-name';
    nameSpan.textContent = group.name;

    const countSpan = document.createElement('span');
    countSpan.className = 'group-chip-count';
    countSpan.textContent = `(${group.files.length})`;

    const delBtn = document.createElement('span');
    delBtn.setAttribute('role', 'button');
    delBtn.tabIndex = 0;
    delBtn.className = 'group-chip-del';
    delBtn.textContent = 'x';
    delBtn.title = `Delete group "${group.name}"`;

    const handleDeleteGroup = async (e) => {
      e.stopPropagation();
      closeGroupContextMenu();
      try {
        const updated = await invoke('delete_batch_group', {
          projectPath: tab.projectPath,
          groupId: group.id,
        });
        tab.groups = Array.isArray(updated) ? updated : [];
        if (tab.activeGroupId === group.id) {
          tab.activeGroupId = null;
        }
        renderGroupsBar(tab);
        applyBatFilterAndRender(tab);
      } catch (err) {
        showAlert('Delete Group Error', escapeHtml(String(err)), '[X]');
      }
    };

    delBtn.addEventListener('click', handleDeleteGroup);
    delBtn.addEventListener('keydown', (e) => {
      if (e.key === 'Enter' || e.key === ' ') {
        e.preventDefault();
        handleDeleteGroup(e);
      }
    });

    const toggleGroupSelection = () => {
      closeBatContextMenu();
      closeGroupContextMenu();
      tab.activeGroupId = tab.activeGroupId === group.id ? null : group.id;
      renderGroupsBar(tab);
      applyBatFilterAndRender(tab);
    };

    chip.addEventListener('click', toggleGroupSelection);
    chip.addEventListener('contextmenu', (e) => {
      e.preventDefault();
      e.stopPropagation();
      closeBatContextMenu();
      showGroupContextMenu(e.clientX, e.clientY, tab, group);
    });
    chip.addEventListener('keydown', (e) => {
      if (e.key === 'Enter' || e.key === ' ') {
        e.preventDefault();
        toggleGroupSelection();
      } else if (e.key === 'ContextMenu' || (e.shiftKey && e.key === 'F10')) {
        e.preventDefault();
        const rect = chip.getBoundingClientRect();
        showGroupContextMenu(rect.left + 8, rect.bottom + 2, tab, group);
      }
    });

    chip.appendChild(nameSpan);
    chip.appendChild(countSpan);
    chip.appendChild(delBtn);
    listEl.appendChild(chip);
  });
}

function showGroupContextMenu(x, y, tab, group) {
  const menu = document.getElementById('group-context-menu');
  if (!menu || !tab || !group) return;

  menu.innerHTML = '';
  const isBusy = activeGroupRunLocked || tab.isGroupRunning || runningGroupId === group.id;
  const item = document.createElement('div');
  item.className = `win95-context-item${isBusy ? ' disabled' : ''}`;
  item.style.fontWeight = 'bold';
  item.textContent = isBusy
    ? `Run All (Running...)`
    : `Run All (${group.files.length})`;

  if (!isBusy) {
    item.addEventListener('click', (e) => {
      e.stopPropagation();
      closeGroupContextMenu();
      runSavedBatchGroup(tab, group);
    });
  }

  menu.appendChild(item);
  positionPopupMenu(menu, x, y);
}

function openAddGroupModal(tab, selectedFiles) {
  if (!tab || !tab.projectPath) {
    showAlert('No Project Selected', 'Please select a project directory before creating a group.', '[!]');
    return;
  }

  const deduped = [];
  const seen = new Set();
  (selectedFiles || []).forEach((f) => {
    if (!f || !f.path || f.isMissing) return;
    const key = f.path.toLowerCase();
    if (!seen.has(key)) {
      seen.add(key);
      deduped.push(f);
    }
  });

  if (deduped.length === 0) {
    showAlert('No Files Selected', 'Please select one or more valid .bat files first.', '[!]');
    return;
  }

  addGroupPendingTabId = tab.id;
  addGroupPendingFiles = deduped;

  const summaryEl = document.getElementById('add-group-files-summary');
  const listEl = document.getElementById('add-group-files-list');
  const radioNew = document.getElementById('add-group-mode-new');
  const radioExisting = document.getElementById('add-group-mode-existing');
  const newNameEl = document.getElementById('add-group-new-name');
  const existingSelEl = document.getElementById('add-group-existing-select');
  const errorEl = document.getElementById('add-group-error');

  if (summaryEl) {
    summaryEl.textContent = `Selected BAT Files (${deduped.length})`;
  }
  if (listEl) {
    listEl.innerHTML = deduped
      .map((f, idx) => `<div>${idx + 1}. ${escapeHtml(f.name)}</div>`)
      .join('');
  }
  if (errorEl) {
    errorEl.style.display = 'none';
    errorEl.textContent = '';
  }

  const groups = tab.groups || [];
  if (existingSelEl) {
    existingSelEl.innerHTML = '';
    if (groups.length === 0) {
      const opt = document.createElement('option');
      opt.value = '';
      opt.textContent = '-- No existing groups --';
      existingSelEl.appendChild(opt);
    } else {
      groups.forEach((g) => {
        const opt = document.createElement('option');
        opt.value = g.id;
        opt.textContent = `${g.name} (${g.files.length} files)`;
        if (tab.activeGroupId && tab.activeGroupId === g.id) {
          opt.selected = true;
        }
        existingSelEl.appendChild(opt);
      });
    }
    existingSelEl.disabled = groups.length === 0;
  }

  if (radioNew && radioExisting) {
    radioNew.checked = true;
    radioExisting.disabled = groups.length === 0;
  }
  if (newNameEl) {
    newNameEl.value = '';
  }

  openSingleModal('modal-add-group');
  if (newNameEl) {
    setTimeout(() => newNameEl.focus(), 20);
  }
}

function showBatContextMenu(x, y, tab) {
  const menu = document.getElementById('bat-context-menu');
  if (!menu || !tab) return;

  const selectedFiles = getSelectedFilesInDisplayOrder(tab);
  if (selectedFiles.length === 0) return;

  closeGroupContextMenu();
  menu.innerHTML = '';

  if (selectedFiles.length >= 2) {
    const isBusy = activeGroupRunLocked || tab.isGroupRunning;
    const runAllItem = document.createElement('div');
    runAllItem.className = `win95-context-item${isBusy ? ' disabled' : ''}`;
    runAllItem.style.fontWeight = 'bold';
    runAllItem.textContent = isBusy
      ? 'Run All (Running...)'
      : `Run All (${selectedFiles.length})`;
    if (!isBusy) {
      runAllItem.addEventListener('click', (e) => {
        e.stopPropagation();
        closeBatContextMenu();
        runBatGroupSequential(tab, selectedFiles, null);
      });
    }
    menu.appendChild(runAllItem);

    const sep = document.createElement('div');
    sep.className = 'win95-context-sep';
    menu.appendChild(sep);
  }

  const addGroupItem = document.createElement('div');
  addGroupItem.className = 'win95-context-item';
  addGroupItem.style.fontWeight = 'bold';
  addGroupItem.textContent = 'ADD GROUP';
  addGroupItem.addEventListener('click', (e) => {
    e.stopPropagation();
    closeBatContextMenu();
    openAddGroupModal(tab, selectedFiles);
  });

  menu.appendChild(addGroupItem);
  positionPopupMenu(menu, x, y);
}

function selectAllVisibleBatFiles(tab = getActiveTab()) {
  if (!tab) return false;
  const displayed = getFilteredBatFiles(tab);
  const validPairs = [];
  displayed.forEach((f, idx) => {
    if (f && f.path && !f.isMissing) {
      validPairs.push({ file: f, idx });
    }
  });
  if (validPairs.length === 0) return false;

  tab.selectedBatPaths = validPairs.map((p) => p.file.path);
  tab.lastSelectedBatIndex = validPairs[0].idx;

  const validSet = new Set(tab.selectedBatPaths.map((p) => p.toLowerCase()));
  const needPreviewLoad =
    !tab.selectedBatPath || !validSet.has(tab.selectedBatPath.toLowerCase());

  if (needPreviewLoad) {
    loadBatFileContentForTab(tab, validPairs[0].file);
  } else if (tab.id === activeTabId) {
    applyBatFilterAndRender(tab);
  } else {
    updateSelectedCountBadge(tab);
  }

  batListActive = true;
  const container = document.getElementById('bat-table-container');
  if (container) {
    try {
      container.focus({ preventScroll: true });
    } catch (_) {}
  }
  return true;
}

function applyBatFilterAndRender(tab = getActiveTab()) {
  if (!tab || tab.id !== activeTabId) return;

  const filterInput = document.getElementById('input-bat-filter');
  if (filterInput) {
    tab.batFilter = filterInput.value;
  }

  const baseFiles = getBaseBatFilesForTab(tab);
  const filtered = getFilteredBatFiles(tab);
  updateSelectedCountBadge(tab);
  renderBatFilesTable(filtered, baseFiles.length, tab);
}

function handleBatRowSelection(e, tab, file, idx, displayedFiles) {
  if (!tab || !file) return;

  batListActive = true;
  const filePathLower = file.path.toLowerCase();
  const isShift = !!e.shiftKey;
  const isCtrl = !!(e.ctrlKey || e.metaKey);

  if (isShift && tab.lastSelectedBatIndex >= 0 && tab.lastSelectedBatIndex < displayedFiles.length) {
    if (window.getSelection) {
      window.getSelection().removeAllRanges();
    }
    const start = Math.min(tab.lastSelectedBatIndex, idx);
    const end = Math.max(tab.lastSelectedBatIndex, idx);
    const rangePaths = displayedFiles
      .slice(start, end + 1)
      .filter((f) => f && f.path && !f.isMissing)
      .map((f) => f.path);

    if (isCtrl) {
      const merged = new Set((tab.selectedBatPaths || []).map((p) => p.toLowerCase()));
      const result = [...(tab.selectedBatPaths || [])];
      rangePaths.forEach((rp) => {
        if (!merged.has(rp.toLowerCase())) {
          merged.add(rp.toLowerCase());
          result.push(rp);
        }
      });
      tab.selectedBatPaths = result;
    } else {
      tab.selectedBatPaths = rangePaths;
    }
  } else if (isCtrl) {
    if (!file.isMissing) {
      const existing = tab.selectedBatPaths || [];
      const foundIdx = existing.findIndex((p) => p.toLowerCase() === filePathLower);
      if (foundIdx >= 0) {
        tab.selectedBatPaths = existing.filter((_, i) => i !== foundIdx);
      } else {
        tab.selectedBatPaths = [...existing, file.path];
      }
      tab.lastSelectedBatIndex = idx;
    }
  } else {
    tab.selectedBatPaths = file.isMissing ? [] : [file.path];
    tab.lastSelectedBatIndex = idx;
  }

  updateSelectedCountBadge(tab);
  if (file.isMissing) {
    tab.selectedBatPath = file.path;
    tab.selectedBatName = file.name;
    tab.selectedBatContent = `[Missing File] Referenced .bat file was not found on disk:\n${file.path}`;
    if (tab.id === activeTabId) {
      applyBatFilterAndRender(tab);
      renderBatPreview(tab);
    }
    return;
  }
  loadBatFileContentForTab(tab, file);
  const container = document.getElementById('bat-table-container');
  if (container) {
    try {
      container.focus({ preventScroll: true });
    } catch (_) {}
  }
}

function renderBatFilesTable(files, totalCount, tab) {
  const tbody = document.getElementById('tbody-bat-files');
  const metaCount = document.getElementById('meta-bat-count');

  if (!totalCount || totalCount === 0) {
    tbody.innerHTML = `
      <tr>
        <td colspan="4" class="empty-row">
          ${tab && tab.activeGroupId ? 'This group has no .bat files.' : 'No .bat files found in target directory. Run Build or browse a custom folder.'}
        </td>
      </tr>
    `;
    metaCount.textContent = '0 files';
    metaCount.className = 'badge badge-gray';
    return;
  }

  if (files.length === totalCount) {
    metaCount.textContent = `${totalCount} files`;
  } else {
    metaCount.textContent = `${files.length} / ${totalCount} files`;
  }
  metaCount.className = files.length > 0 ? 'badge badge-green' : 'badge badge-yellow';

  if (files.length === 0) {
    tbody.innerHTML = `
      <tr>
        <td colspan="4" class="empty-row">
          No .bat files match filter.
        </td>
      </tr>
    `;
    return;
  }

  tbody.innerHTML = '';
  const selectedSet = new Set(
    ((tab && tab.selectedBatPaths) || []).map((p) => p.toLowerCase())
  );
  if (selectedSet.size === 0 && tab && tab.selectedBatPath) {
    selectedSet.add(tab.selectedBatPath.toLowerCase());
  }

  files.forEach((file, idx) => {
    const tr = document.createElement('tr');
    const isSelected = selectedSet.has(file.path.toLowerCase());
    if (isSelected) {
      tr.classList.add('selected-row');
    }
    if (file.isMissing) {
      tr.classList.add('bat-row-missing');
    }

    const execStatus = getBatExecStatus(tab, file.path);
    const execMeta =
      tab && tab.batExecMeta ? tab.batExecMeta[String(file.path || '').toLowerCase()] : null;
    let statusTagHtml = '';
    let statusNote = '';
    if (execStatus === 'success') {
      tr.classList.add('bat-status-success');
      statusTagHtml = '<span class="bat-status-tag bat-tag-success">OK</span>';
      statusNote =
        execMeta && execMeta.exit_code !== null && execMeta.exit_code !== undefined
          ? ` | Last status: SUCCESS (Exit ${execMeta.exit_code})`
          : ' | Last status: SUCCESS';
    } else if (execStatus === 'failed') {
      tr.classList.add('bat-status-failed');
      statusTagHtml = '<span class="bat-status-tag bat-tag-failed">FAIL</span>';
      statusNote =
        execMeta && execMeta.exit_code !== null && execMeta.exit_code !== undefined
          ? ` | Last status: FAILED (Exit ${execMeta.exit_code})`
          : ' | Last status: FAILED';
    } else if (execStatus === 'pending') {
      tr.classList.add('bat-status-pending');
      statusTagHtml = '<span class="bat-status-tag bat-tag-pending">WAIT</span>';
      statusNote = ' | Status: WAITING / PENDING';
    } else if (execStatus === 'running') {
      tr.classList.add('bat-status-running');
      statusTagHtml = '<span class="bat-status-tag bat-tag-running">RUN</span>';
      statusNote = ' | Status: RUNNING';
    }

    tr.title = file.isMissing
      ? `Missing file: ${file.path}${statusNote}`
      : `Click to preview | Ctrl/Shift+Click or Ctrl+A multi-select | Right-click ADD GROUP / Run All | Dbl-click Run: ${file.path}${statusNote}`;

    tr.innerHTML = `
      <td style="text-align: center;">${idx + 1}</td>
      <td title="${escapeHtml((file.path || file.name) + statusNote)}">
        <div class="file-name-cell">
          <span class="file-name-text"><b>${escapeHtml(file.name)}</b></span>${statusTagHtml}${file.isMissing ? ' <span class="badge badge-red">MISSING</span>' : ''}
        </div>
      </td>
      <td style="text-align: center; font-size: 10px;">${escapeHtml(file.modified_time)}</td>
      <td style="text-align: center;">
        <button class="win95-btn btn-bat-run" data-path="${escapeHtml(file.path)}"${file.isMissing ? ' disabled' : ''}>
          ${execStatus === 'running' ? 'Run...' : 'Run'}
        </button>
      </td>
    `;

    tr.tabIndex = 0;
    tr.addEventListener('click', (e) => {
      closeBatContextMenu();
      closeGroupContextMenu();
      const activeTab = getActiveTab();
      if (activeTab) {
        handleBatRowSelection(e, activeTab, file, idx, files);
      }
    });

    tr.addEventListener('keydown', (e) => {
      const activeTab = getActiveTab();
      if (!activeTab) return;
      if (e.key === 'Enter' || e.key === ' ') {
        e.preventDefault();
        handleBatRowSelection(e, activeTab, file, idx, files);
      } else if ((e.key === 'ContextMenu' || (e.shiftKey && e.key === 'F10')) && !file.isMissing) {
        e.preventDefault();
        const curSet = new Set((activeTab.selectedBatPaths || []).map((p) => p.toLowerCase()));
        if (!curSet.has(file.path.toLowerCase())) {
          activeTab.selectedBatPaths = [file.path];
          activeTab.lastSelectedBatIndex = idx;
          updateSelectedCountBadge(activeTab);
          loadBatFileContentForTab(activeTab, file);
        }
        const rect = tr.getBoundingClientRect();
        showBatContextMenu(rect.left + 20, rect.bottom, activeTab);
      }
    });

    tr.addEventListener('dblclick', () => {
      closeBatContextMenu();
      closeGroupContextMenu();
      if (file.isMissing) return;
      const activeTab = getActiveTab();
      if (activeTab) {
        activeTab.selectedBatPaths = [file.path];
        activeTab.lastSelectedBatIndex = idx;
        updateSelectedCountBadge(activeTab);
        loadBatFileContentForTab(activeTab, file);
      }
      runBatScript(file.path);
    });

    tr.addEventListener('contextmenu', (e) => {
      e.preventDefault();
      e.stopPropagation();
      closeGroupContextMenu();
      const activeTab = getActiveTab();
      if (!activeTab || file.isMissing) return;

      const curSet = new Set((activeTab.selectedBatPaths || []).map((p) => p.toLowerCase()));
      if (!curSet.has(file.path.toLowerCase())) {
        activeTab.selectedBatPaths = [file.path];
        activeTab.lastSelectedBatIndex = idx;
        updateSelectedCountBadge(activeTab);
        loadBatFileContentForTab(activeTab, file);
      }
      showBatContextMenu(e.clientX, e.clientY, activeTab);
    });

    tbody.appendChild(tr);
  });

  tbody.querySelectorAll('.btn-bat-run').forEach((btn) => {
    btn.addEventListener('click', (e) => {
      e.stopPropagation();
      closeBatContextMenu();
      closeGroupContextMenu();
      if (btn.disabled) return;
      const p = btn.getAttribute('data-path');
      runBatScript(p);
    });
  });
}

async function runBatScript(path) {
  const tab = getActiveTab();
  if (tab && path) {
    setBatExecStatus(tab, path, 'running', null);
    if (tab.id === activeTabId) {
      applyBatFilterAndRender(tab);
    }
  }
  try {
    const res = await invoke('execute_bat_file', {
      tabId: tab ? tab.id : null,
      path,
      projectPath: tab && tab.projectPath ? tab.projectPath : null,
    });
    if (res && res.screenshot_error) {
      showAlert(
        'Auto Save Image Warning',
        `Batch script was launched, but screenshot auto-save could not be prepared:<br>${escapeHtml(res.screenshot_error)}`,
        '[!]'
      );
    }
  } catch (err) {
    if (tab && path) {
      setBatExecStatus(tab, path, 'failed', { error_message: String(err) });
      if (tab.id === activeTabId) {
        applyBatFilterAndRender(tab);
      }
    }
    showAlert('Execution Error', `Failed to run script: ${escapeHtml(err)}`, '[X]');
  }
}

async function openBatInTextEditor(path) {
  try {
    await invoke('open_file_in_text_editor', { path });
  } catch (err) {
    showAlert('Open File Error', `Failed to open file in text editor: ${escapeHtml(err)}`, '[X]');
  }
}

function renderGroupRunModal() {
  const tbody = document.getElementById('tbody-group-run-results');
  const outTitle = document.getElementById('group-run-output-title');
  const outLog = document.getElementById('group-run-output-log');
  if (!tbody) return;

  tbody.innerHTML = '';
  groupRunSteps.forEach((step, idx) => {
    const tr = document.createElement('tr');
    if (step.status === 'RUNNING') {
      tr.classList.add('bat-status-running');
    } else if (step.status === 'OK') {
      tr.classList.add('bat-status-success');
    } else if (step.status === 'FAILED') {
      tr.classList.add('bat-status-failed');
    } else {
      tr.classList.add('bat-status-pending');
    }
    if (idx === groupRunSelectedIdx) {
      tr.classList.add('selected-row');
    }

    let badgeHtml = '<span class="badge badge-gray">PENDING</span>';
    if (step.status === 'RUNNING') {
      badgeHtml = '<span class="badge badge-yellow">RUNNING</span>';
    } else if (step.status === 'OK') {
      badgeHtml = '<span class="badge badge-green">OK</span>';
    } else if (step.status === 'FAILED') {
      badgeHtml = '<span class="badge badge-red">FAILED</span>';
    }

    let imgBadgeHtml = '<span class="badge badge-gray">-</span>';
    if (step.screenshot_saved) {
      imgBadgeHtml = '<span class="badge badge-green" title="PNG screenshot saved">SAVED</span>';
    } else if (step.screenshot_error) {
      imgBadgeHtml = `<span class="badge badge-red" title="${escapeHtml(step.screenshot_error)}">ERR</span>`;
    }

    const exitText =
      step.exit_code !== null && step.exit_code !== undefined ? String(step.exit_code) : '-';
    const timeText = step.duration_ms !== null ? `${step.duration_ms} ms` : '-';

    tr.innerHTML = `
      <td style="text-align: center;">${idx + 1}</td>
      <td title="${escapeHtml(step.path)}"><b>${escapeHtml(step.name)}</b></td>
      <td style="text-align: center;">${badgeHtml}</td>
      <td style="text-align: center;">${escapeHtml(exitText)}</td>
      <td style="text-align: right;">${escapeHtml(timeText)}</td>
      <td style="text-align: center;">${imgBadgeHtml}</td>
    `;

    tr.addEventListener('click', () => {
      groupRunSelectedIdx = idx;
      renderGroupRunModal();
    });

    tbody.appendChild(tr);
  });

  const activeStep = groupRunSteps[groupRunSelectedIdx] || null;
  const openImgBtn = document.getElementById('modal-btn-open-group-image');
  if (openImgBtn) {
    openImgBtn.disabled = !(activeStep && activeStep.screenshot_saved && activeStep.screenshot_path);
    openImgBtn.title =
      activeStep && activeStep.screenshot_saved && activeStep.screenshot_path
        ? `Open ${activeStep.screenshot_path}`
        : 'Open saved PNG screenshot for selected step';
  }
  if (outTitle && outLog) {
    if (!activeStep) {
      outTitle.textContent = 'Execution Output';
      outLog.textContent = 'Select a row above to inspect its stdout/stderr.';
    } else {
      outTitle.textContent = `Execution Output: ${activeStep.name} [${activeStep.status}]`;
      const parts = [];
      if (activeStep.error_message) {
        parts.push(`[ERROR] ${activeStep.error_message}`);
      }
      if (activeStep.screenshot_saved && activeStep.screenshot_path) {
        parts.push(`[IMAGE SAVED] ${activeStep.screenshot_path}`);
      } else if (activeStep.screenshot_error) {
        parts.push(`[IMAGE ERROR] ${activeStep.screenshot_error}`);
      }
      if (activeStep.stdout && activeStep.stdout.trim() !== '') {
        parts.push(activeStep.stdout);
      }
      if (activeStep.stderr && activeStep.stderr.trim() !== '') {
        parts.push(`--- STDERR ---\n${activeStep.stderr}`);
      }
      outLog.textContent =
        parts.length > 0
          ? parts.join('\n')
          : activeStep.status === 'RUNNING'
            ? 'Executing sequentially...'
            : '(No output)';
    }
  }
}

function runSavedBatchGroup(tab, group) {
  if (!tab || !group) return;
  const orderedFiles = resolveGroupOrderedFiles(tab, group);
  if (orderedFiles.length === 0) {
    showAlert('Empty Group', `Group "${escapeHtml(group.name)}" contains no BAT files.`, '[!]');
    return;
  }
  runBatGroupSequential(tab, orderedFiles, group);
}

async function runBatGroupSequential(tab, filesInOrder, groupMeta = null) {
  if (!tab || !filesInOrder || filesInOrder.length === 0) return;
  if (activeGroupRunLocked || tab.isGroupRunning) {
    showAlert('Group Run Busy', 'A batch group execution is already in progress.', '[!]');
    return;
  }

  activeGroupRunLocked = true;
  tab.isGroupRunning = true;
  runningGroupId = groupMeta ? groupMeta.id : null;
  renderGroupsBar(tab);

  const modalTitleEl = document.querySelector('#modal-group-run .title-bar-text');
  if (modalTitleEl) {
    modalTitleEl.textContent = groupMeta
      ? `Run Group - ${groupMeta.name}`
      : `Run All - Selected BAT Files (${filesInOrder.length})`;
  }

  const summaryEl = document.getElementById('group-run-summary');
  const closeBtn = document.getElementById('modal-btn-close-group-run');
  const closeXBtn = document.getElementById('btn-close-modal-group-run');
  if (closeBtn) closeBtn.disabled = true;
  if (closeXBtn) closeXBtn.disabled = true;

  groupRunSteps = filesInOrder.map((f) => ({
    name: f.name,
    path: f.path,
    status: 'PENDING',
    exit_code: null,
    duration_ms: null,
    stdout: '',
    stderr: '',
    error_message: null,
    screenshot_path: null,
    screenshot_saved: false,
    screenshot_error: null,
  }));
  groupRunSelectedIdx = 0;

  // Reset all queued files in this batch run to WAITING / PENDING (Light Yellow)
  filesInOrder.forEach((f) => {
    if (f && f.path) {
      setBatExecStatus(tab, f.path, 'pending', null);
    }
  });
  if (tab.id === activeTabId) {
    applyBatFilterAndRender(tab);
  }

  openSingleModal('modal-group-run');
  renderGroupRunModal();

  let passedCount = 0;
  let failedCount = 0;
  let imageErrorCount = 0;
  const total = filesInOrder.length;
  const groupPrefix = groupMeta ? `[${groupMeta.name}] ` : '';

  try {
    for (let i = 0; i < total; i++) {
      const file = filesInOrder[i];
      groupRunSteps[i].status = 'RUNNING';
      groupRunSelectedIdx = i;
      setBatExecStatus(tab, file.path, 'running', null);
      if (tab.id === activeTabId) {
        applyBatFilterAndRender(tab);
      }
      if (summaryEl) {
        summaryEl.textContent = `${groupPrefix}Running step ${i + 1} of ${total}: ${file.name}...`;
      }
      renderGroupRunModal();

      try {
        const stepRes = await invoke('execute_bat_group_step', {
          path: file.path,
          projectPath: tab.projectPath || null,
        });
        groupRunSteps[i].duration_ms = stepRes.duration_ms;
        groupRunSteps[i].exit_code = stepRes.exit_code;
        groupRunSteps[i].stdout = stepRes.stdout || '';
        groupRunSteps[i].stderr = stepRes.stderr || '';
        groupRunSteps[i].error_message = stepRes.error_message || null;
        groupRunSteps[i].screenshot_path = stepRes.screenshot_path || null;
        groupRunSteps[i].screenshot_saved = !!stepRes.screenshot_saved;
        groupRunSteps[i].screenshot_error = stepRes.screenshot_error || null;
        groupRunSteps[i].status = stepRes.success ? 'OK' : 'FAILED';

        setBatExecStatus(tab, file.path, stepRes.success ? 'success' : 'failed', stepRes);

        if (stepRes.screenshot_error) {
          imageErrorCount += 1;
        }

        if (stepRes.success) {
          passedCount += 1;
          appendBuildLogForTab(
            tab.id,
            `[Group ${i + 1}/${total}] OK: ${file.name} (${stepRes.duration_ms} ms)${stepRes.screenshot_saved ? ' [PNG saved]' : ''}`
          );
        } else {
          failedCount += 1;
          appendBuildLogForTab(
            tab.id,
            `[Group ${i + 1}/${total}] FAILED: ${file.name} - ${stepRes.error_message || 'Non-zero exit'}`
          );
        }
      } catch (err) {
        failedCount += 1;
        groupRunSteps[i].status = 'FAILED';
        groupRunSteps[i].duration_ms = 0;
        groupRunSteps[i].error_message = String(err);
        setBatExecStatus(tab, file.path, 'failed', { error_message: String(err) });
        appendBuildLogForTab(
          tab.id,
          `[Group ${i + 1}/${total}] ERROR: ${file.name} - ${err}`
        );
      }

      if (tab.id === activeTabId) {
        applyBatFilterAndRender(tab);
      }
      renderGroupRunModal();
    }
  } finally {
    activeGroupRunLocked = false;
    tab.isGroupRunning = false;
    runningGroupId = null;
    renderGroupsBar(tab);
    if (closeBtn) closeBtn.disabled = false;
    if (closeXBtn) closeXBtn.disabled = false;
    if (summaryEl) {
      const imgNote =
        imageErrorCount > 0 ? ` (${imageErrorCount} screenshot error(s))` : '';
      summaryEl.textContent = `${groupPrefix}Completed ${total} file(s) sequentially: ${passedCount} succeeded, ${failedCount} failed.${imgNote}`;
    }
  }
}

// ==========================================================================
// 4. BUILD EXECUTION (PER TAB)
// ==========================================================================
async function executeBuildForActiveTab() {
  const tab = getActiveTab();
  if (!tab || !tab.projectPath) {
    showAlert('No Project Selected', 'Please select a project directory before building.', '[!]');
    return;
  }

  const targetSelect = document.getElementById('select-ant-target');
  const target = targetSelect ? targetSelect.value : tab.selectedTarget;
  if (!target) {
    showAlert('No Target Selected', 'Please select an Ant target to build.', '[!]');
    return;
  }

  tab.selectedTarget = target;

  const inputTargetEl = document.getElementById('target-build-path-input');
  if (inputTargetEl && inputTargetEl.dataset.tabId === tab.id) {
    const typedVal = (inputTargetEl.value || '').trim();
    const currentEff = (tab.effectiveTargetPath || '').trim();
    if (typedVal.replace(/\//g, '\\').toLowerCase() !== currentEff.replace(/\//g, '\\').toLowerCase()) {
      try {
        await applyTargetBuildPathForActiveTab(typedVal, {
          resetToDefault: typedVal === '',
          explicitTab: tab,
        });
      } catch (err) {
        showAlert('Target Path Error', escapeHtml(String(err)), '[X]');
        return;
      }
    }
  }

  const validation = await refreshTargetPathValidationForTab(tab);
  if (validation && !validation.is_valid) {
    setTabFooterStatus(tab, 'Build blocked (Target Path)', formatCurrentDateTime(), 'err');
    openTargetPathWarningModal(validation);
    return;
  }

  tab.isBuilding = true;
  tab.cmdPanelOpen = true;
  tab.buildStatus = 'BUILDING...';
  tab.buildStatusClass = 'badge-yellow';
  tab.consoleLogs = '';

  tab.footerStatusText = 'Building...';
  tab.footerStatusTime = '';
  tab.footerStatusKind = 'neutral';

  renderTabsBar();
  syncUIToActiveTab();

  const buildProjectPath = tab.projectPath;
  const buildTarget = target;
  const buildCustomTarget = tab.customTargetPath || null;
  const buildSeq = tab.requestSeq || 0;

  try {
    const res = await invoke('start_ant_build', {
      tabId: tab.id,
      projectPath: buildProjectPath,
      target: buildTarget,
      antExecutable: selectedAntPath,
      javaHome: selectedJavaHome,
      customTargetPath: buildCustomTarget,
    });

    if (
      tab.projectPath === buildProjectPath &&
      tab.selectedTarget === buildTarget &&
      (tab.requestSeq || 0) === buildSeq
    ) {
      tab.propertiesFile = res.properties_file || tab.propertiesFile || null;
      if (res.default_target_path) tab.defaultTargetPath = res.default_target_path;
      if (res.effective_target_path) tab.effectiveTargetPath = res.effective_target_path;
      tab.batFolder = res.bat_folder || '';
      tab.batchLogName = res.batch_log_name || tab.batchLogName || null;
      tab.appLogName = res.app_log_name || tab.appLogName || null;
      tab.logFile = res.log_file_path || null;
      tab.logFolder = res.log_folder_path || null;
      tab.batFiles = res.bat_files || [];

      pruneTabBatSelections(tab);

      if (tab.selectedBatPath) {
        const matched = tab.batFiles.find(
          (f) => f.path.toLowerCase() === tab.selectedBatPath.toLowerCase()
        );
        if (matched) {
          await loadBatFileContentForTab(tab, matched);
        }
      }

      await refreshDatabaseInfoForTab(tab, buildSeq);
    }

    if (res.success) {
      const doneAt = formatCurrentDateTime();
      tab.buildStatus = '\u2714 BUILD SUCCESSFUL';
      tab.buildStatusClass = 'badge-green';
      setTabFooterStatus(tab, 'Build Success', doneAt, 'ok');
    } else {
      const doneAt = formatCurrentDateTime();
      tab.buildStatus = '\u2716 BUILD FAILED';
      tab.buildStatusClass = 'badge-red';
      setTabFooterStatus(tab, 'Build Failed', doneAt, 'err');
    }
  } catch (err) {
    console.error('Build execution failed:', err);
    tab.buildStatus = '\u2716 BUILD ERROR';
    tab.buildStatusClass = 'badge-red';
    setTabFooterStatus(tab, 'Build Error', formatCurrentDateTime(), 'err');
    appendBuildLogForTab(tab.id, `[ERROR] ${err}`);
    if (tab.id === activeTabId) {
      showAlert('Build Error', `Failed to execute build: ${err}`, '[X]');
    }
  } finally {
    tab.isBuilding = false;
    renderTabsBar();
    if (tab.id === activeTabId) {
      syncUIToActiveTab();
    }
  }
}

// ==========================================================================
// 5. ENVIRONMENT (ANT & JAVA) CHECK & MODALS
// ==========================================================================
function renderEnvironmentUI(env) {
  envData = env;

  // Footer Ant
  const footerAnt = document.getElementById('footer-ant-val');
  const antWhere =
    env.ant_where_path ||
    (env.ant_executables && env.ant_executables.length > 0 ? env.ant_executables.join(', ') : '') ||
    (env.where_ant_cmd && env.where_ant_cmd.stdout ? env.where_ant_cmd.stdout : '');

  if (env.ant_installed && env.ant_version) {
    footerAnt.textContent = env.ant_version
      .split('\n')[0]
      .replace('Apache Ant(TM) version ', '')
      .replace(/ compiled on.*/i, '');
    footerAnt.style.color = '#000000';
    footerAnt.title = `Path: ${antWhere || 'N/A'}\nANT_HOME: ${env.ant_home || 'Not set'}`;
  } else {
    footerAnt.textContent = 'Not installed';
    footerAnt.style.color = '#aa0000';
  }

  // Footer Java
  const footerJava = document.getElementById('footer-java-val');
  const javaWhere =
    env.java_where_path ||
    (env.java_executables && env.java_executables.length > 0 ? env.java_executables.join(', ') : '') ||
    (env.where_java_cmd && env.where_java_cmd.stdout ? env.where_java_cmd.stdout : '');

  if (env.java_installed && env.java_version) {
    footerJava.textContent = env.java_version
      .split('\n')[0]
      .replace('java version ', '')
      .replace('openjdk version ', '')
      .replace(/"/g, '');
    footerJava.style.color = '#000000';
    footerJava.title = `Path: ${javaWhere || 'N/A'}\nJAVA_HOME: ${env.java_home || 'Not set'}`;
  } else {
    footerJava.textContent = 'Not installed';
    footerJava.style.color = '#aa0000';
  }

  // Modal Ant info
  document.getElementById('modal-ant-status').innerHTML = env.ant_installed
    ? '<span class="badge badge-green">INSTALLED</span>'
    : '<span class="badge badge-red">NOT INSTALLED</span>';
  document.getElementById('modal-ant-version').textContent = env.ant_version || 'Not detected';
  document.getElementById('modal-ant-path').textContent = antWhere || 'Not in PATH';
  document.getElementById('modal-ant-home').textContent = env.ant_home || '(ANT_HOME not set)';

  // Modal Java info
  document.getElementById('modal-java-status').innerHTML = env.java_installed
    ? '<span class="badge badge-green">INSTALLED</span>'
    : '<span class="badge badge-red">NOT INSTALLED</span>';
  document.getElementById('modal-java-version').textContent = env.java_version || 'Not detected';
  document.getElementById('modal-javac-version').textContent = env.javac_version || 'Not detected';
  document.getElementById('modal-java-path').textContent = javaWhere || 'Not in PATH';
  document.getElementById('modal-java-home').textContent = env.java_home || '(JAVA_HOME not set)';

  syncSelectedJdkToEnv();
}

let userManuallyPickedJdk = false;

function syncSelectedJdkToEnv() {
  if (userManuallyPickedJdk || !envData || !envData.java_home) return;
  const targetHome = envData.java_home.trim();
  if (!targetHome) return;

  selectedJavaHome = targetHome;
  const select = document.getElementById('modal-select-jdk');
  const descEl = document.getElementById('modal-jdk-choice-desc');
  if (select && jdkScanData && Array.isArray(jdkScanData.jdks)) {
    const matched = jdkScanData.jdks.find(
      (j) => j.root_path && j.root_path.toLowerCase() === targetHome.toLowerCase()
    );
    if (matched) {
      select.value = matched.id;
    }
  }
  if (descEl) {
    descEl.textContent = `Path: ${targetHome}`;
    descEl.title = targetHome;
  }
}

async function loadEnvironment() {
  try {
    const env = await invoke('check_system_environment');
    renderEnvironmentUI(env);
  } catch (err) {
    console.error('Check environment failed:', err);
  }
}

async function loadAntOptions() {
  try {
    const res = await invoke('get_available_ants');
    antScanData = res;

    const select = document.getElementById('modal-select-ant');
    const descEl = document.getElementById('modal-ant-choice-desc');
    select.innerHTML = '';

    res.available_options.forEach((opt, idx) => {
      const el = document.createElement('option');
      el.value = opt.id;
      let shortLabel = opt.name;
      if (opt.source_type === 'bundled_zip') {
        shortLabel = `Apache Ant ${opt.version || '1.10.18'} (Bundled ZIP in libs)`;
      } else if (opt.source_type === 'bundled_extracted') {
        shortLabel = `Apache Ant ${opt.version || '1.10.18'} (Extracted in libs)`;
      } else if (opt.source_type === 'system') {
        const cleanVer = (opt.version || 'PATH')
          .replace('Apache Ant(TM) version ', '')
          .replace(/ compiled on.*/i, '');
        shortLabel = `System Ant (${cleanVer})`;
      }
      el.textContent = shortLabel;
      el.title = opt.executable_path || opt.root_path || opt.name;
      if (idx === 0) el.selected = true;
      select.appendChild(el);
    });

    const btnExtract = document.getElementById('modal-btn-extract-ant');
    const updateAntSelection = () => {
      const selected = res.available_options.find((o) => o.id === select.value);
      if (selected && selected.source_type === 'bundled_zip' && !selected.is_ready_to_use) {
        btnExtract.style.display = 'inline-block';
        btnExtract.setAttribute('data-zip', selected.root_path);
      } else {
        btnExtract.style.display = 'none';
      }

      if (selected) {
        selectedAntPath = selected.is_ready_to_use ? selected.executable_path : null;
        selectedAntRootOrZip = selected.root_path || selected.executable_path || null;
        if (descEl) {
          descEl.textContent = `Path: ${selected.executable_path || selected.root_path || '(System PATH)'}`;
          descEl.title = descEl.textContent;
        }
      }
    };

    select.onchange = updateAntSelection;
    updateAntSelection();
  } catch (err) {
    console.error('Scan ants failed:', err);
  }
}

async function loadJdkOptions() {
  try {
    const res = await invoke('get_available_jdks');
    jdkScanData = res;

    const select = document.getElementById('modal-select-jdk');
    const descEl = document.getElementById('modal-jdk-choice-desc');
    select.innerHTML = '';

    res.jdks.forEach((jdk) => {
      const opt = document.createElement('option');
      opt.value = jdk.id;
      opt.textContent = jdk.name;
      opt.title = jdk.root_path;
      if (jdk.is_recommended || jdk.id === res.recommended_jdk_id) {
        opt.selected = true;
        selectedJavaHome = jdk.root_path;
      }
      select.appendChild(opt);
    });

    const updateJdkSelection = (isUserEvent = false) => {
      const chosen = res.jdks.find((j) => j.id === select.value);
      if (chosen && chosen.root_path) {
        if (isUserEvent) userManuallyPickedJdk = true;
        selectedJavaHome = chosen.root_path;
        if (descEl) {
          descEl.textContent = `Path: ${chosen.root_path}`;
          descEl.title = chosen.root_path;
        }
      }
    };

    select.onchange = () => updateJdkSelection(true);
    updateJdkSelection(false);
    syncSelectedJdkToEnv();
  } catch (err) {
    console.error('Scan JDKs failed:', err);
  }
}

async function loadUserSetting() {
  try {
    const [setting, settingPath, defaultImgDir] = await Promise.all([
      invoke('get_user_setting'),
      invoke('get_user_setting_path'),
      invoke('get_default_screenshot_dir').catch(() => ''),
    ]);
    if (setting) {
      userSetting = setting;
    }
    if (defaultImgDir) {
      defaultScreenshotRootDir = defaultImgDir;
    }
    updateFooterConfigVal();
    const descEl = document.getElementById('modal-setting-file-desc');
    if (descEl && settingPath) {
      descEl.textContent = `File: ${settingPath}`;
      descEl.title = settingPath;
    }
    const hintEl = document.getElementById('setting-screenshot-default-hint');
    if (hintEl && defaultImgDir) {
      hintEl.textContent = `Default Root: ${defaultImgDir}\\{project}\\{bat}.png (overwrites existing)`;
      hintEl.title = hintEl.textContent;
    }
  } catch (err) {
    console.error('Load user setting failed:', err);
  }
}

function updateFooterConfigVal() {
  const valEl = document.getElementById('footer-config-val');
  if (valEl) {
    const capText = userSetting.auto_capture_clipboard ? 'Shot:ON' : 'Shot:OFF';
    const imgText = userSetting.auto_save_image ? 'Img:ON' : 'Img:OFF';
    valEl.textContent = `${userSetting.ps_window_width}x${userSetting.ps_window_height} (${capText} | ${imgText})`;
  }
}

function updateScreenshotToggleBadge() {
  const capEl = document.getElementById('setting-auto-capture');
  const badgeEl = document.getElementById('setting-auto-capture-state');
  if (capEl && badgeEl) {
    const enabled = !!capEl.checked;
    badgeEl.textContent = enabled ? 'ON' : 'OFF';
    badgeEl.className = `badge ${enabled ? 'badge-green' : 'badge-gray'}`;
  }

  const saveImgEl = document.getElementById('setting-auto-save-image');
  const saveImgBadge = document.getElementById('setting-auto-save-image-state');
  if (saveImgEl && saveImgBadge) {
    const enabled = !!saveImgEl.checked;
    saveImgBadge.textContent = enabled ? 'ON' : 'OFF';
    saveImgBadge.className = `badge ${enabled ? 'badge-green' : 'badge-gray'}`;
  }

  const unsavedEl = document.getElementById('setting-unsaved-note');
  if (unsavedEl) {
    const curSaveImg = saveImgEl ? !!saveImgEl.checked : false;
    const curCap = capEl ? !!capEl.checked : false;
    const curDirRaw = (document.getElementById('setting-screenshot-dir')?.value || '').trim();
    const savedDirNorm = (userSetting.screenshot_save_dir || defaultScreenshotRootDir || '')
      .replace(/[\\/]+$/, '')
      .toLowerCase();
    const curDirNorm = curDirRaw.replace(/[\\/]+$/, '').toLowerCase();
    const hasChanges =
      curSaveImg !== !!userSetting.auto_save_image ||
      curCap !== !!userSetting.auto_capture_clipboard ||
      (curDirNorm !== savedDirNorm && !(curDirNorm === '' && !userSetting.screenshot_save_dir));
    if (hasChanges) {
      unsavedEl.style.display = 'block';
      unsavedEl.textContent = 'Unsaved change(s): click [Save] below to apply and persist to setting.json.';
    } else {
      unsavedEl.style.display = 'none';
      unsavedEl.textContent = '';
    }
  }
}

function populateSettingsModal() {
  const widthEl = document.getElementById('setting-ps-width');
  const heightEl = document.getElementById('setting-ps-height');
  const capEl = document.getElementById('setting-auto-capture');
  const saveImgEl = document.getElementById('setting-auto-save-image');
  const saveDirEl = document.getElementById('setting-screenshot-dir');
  if (widthEl) widthEl.value = userSetting.ps_window_width;
  if (heightEl) heightEl.value = userSetting.ps_window_height;
  if (capEl) capEl.checked = !!userSetting.auto_capture_clipboard;
  if (saveImgEl) saveImgEl.checked = !!userSetting.auto_save_image;
  if (saveDirEl) {
    saveDirEl.value = userSetting.screenshot_save_dir || defaultScreenshotRootDir || '';
  }
  updateScreenshotToggleBadge();
}

// ==========================================================================
// 5b. AUTO CONFIG CHECKLIST MODAL, DIRECT SHOW CONFIG & SPLIT RESIZER
// ==========================================================================
async function openConfigPropertiesDirect() {
  const tab = getActiveTab();
  if (!tab || !tab.projectPath || !tab.selectedTarget) {
    showAlert('No Target Selected', 'Please select a project and target package first.', '[!]');
    return;
  }

  let propFile = tab.propertiesFile;
  if (!propFile) {
    try {
      const inspection = await invoke('inspect_build_target', {
        projectPath: tab.projectPath,
        target: tab.selectedTarget,
        customTargetPath: tab.customTargetPath || null,
      });
      tab.propertiesFile = inspection.properties_file || null;
      propFile = tab.propertiesFile;
    } catch (err) {
      console.error('Inspect config failed:', err);
    }
  }

  if (!propFile) {
    showAlert(
      'Config Not Found',
      `No .properties file found for target "${escapeHtml(tab.selectedTarget)}".`,
      '[!]'
    );
    return;
  }

  try {
    await invoke('open_file_default', { path: propFile });
  } catch (err) {
    showAlert(
      'Open Config Error',
      `Failed to open properties file: ${escapeHtml(err)}`,
      '[X]'
    );
  }
}

async function openAutoConfigModal() {
  openSingleModal('modal-auto-config');
  await refreshAutoConfigChecks();
}

async function refreshAutoConfigChecks() {
  const antBadge = document.getElementById('autoconfig-check-ant-badge');
  const antDetail = document.getElementById('autoconfig-check-ant-detail');
  const javaBadge = document.getElementById('autoconfig-check-java-badge');
  const javaDetail = document.getElementById('autoconfig-check-java-detail');
  const winBadge = document.getElementById('autoconfig-check-window-badge');
  const winDetail = document.getElementById('autoconfig-check-window-detail');
  const projEl = document.getElementById('autoconfig-project-path');
  const tgtEl = document.getElementById('autoconfig-target-name');
  const tgtDirEl = document.getElementById('autoconfig-target-dir');
  const javaHomeEl = document.getElementById('autoconfig-java-home');
  const propEl = document.getElementById('autoconfig-prop-file');
  const summaryEl = document.getElementById('autoconfig-summary-note');
  const applyBtn = document.getElementById('modal-btn-apply-autoconfig');

  const tab = getActiveTab();
  const hasTarget = !!(tab && tab.projectPath && tab.selectedTarget);

  if (projEl) {
    projEl.textContent = (tab && tab.projectPath) || '(No project selected)';
    projEl.title = projEl.textContent;
  }
  if (tgtEl) tgtEl.textContent = (tab && tab.selectedTarget) || '(None)';
  if (tgtDirEl) {
    const effDir = tab
      ? tab.effectiveTargetPath || tab.defaultTargetPath || (tab.projectPath ? `${tab.projectPath}\\target` : '-')
      : '-';
    const modeSuffix = tab && tab.projectPath ? (tab.customTargetPath ? ' (Custom)' : ' (Default)') : '';
    tgtDirEl.textContent = tab && tab.projectPath ? `${effDir}${modeSuffix}` : '-';
    tgtDirEl.title = effDir;
  }
  if (javaHomeEl) javaHomeEl.textContent = 'Checking...';
  if (propEl) propEl.textContent = 'Checking...';
  if (summaryEl) summaryEl.textContent = 'Checking Ant, Java, and Window Size settings...';

  try {
    const envPromise = invoke('check_system_environment');
    const settingPromise = invoke('get_user_setting');
    const inspectPromise = hasTarget
      ? invoke('inspect_build_target', {
          projectPath: tab.projectPath,
          target: tab.selectedTarget,
          customTargetPath: tab.customTargetPath || null,
        })
      : Promise.resolve(null);

    const [env, setting, inspection] = await Promise.all([
      envPromise,
      settingPromise,
      inspectPromise,
    ]);

    if (env) {
      renderEnvironmentUI(env);
    }
    if (setting) {
      userSetting = setting;
      updateFooterConfigVal();
    }
    if (tab && inspection) {
      tab.propertiesFile = inspection.properties_file || null;
      tab.batchLogName = inspection.batch_log_name || null;
      tab.appLogName = inspection.app_log_name || null;
      if (tab.id === activeTabId) {
        updateLogButtonsUI(tab);
      }
    }

    // Check 1: Ant
    const antOk = !!(env && env.ant_installed) || !!selectedAntPath;
    if (antBadge) {
      antBadge.textContent = antOk ? 'OK' : 'MISSING';
      antBadge.className = `badge ${antOk ? 'badge-green' : 'badge-red'}`;
    }
    if (antDetail) {
      if (env && env.ant_installed) {
        const ver = (env.ant_version || '')
          .split('\n')[0]
          .replace('Apache Ant(TM) version ', '')
          .replace(/ compiled on.*/i, '');
        antDetail.textContent = `${ver || 'Installed'} | ${env.ant_home || env.ant_where_path || 'PATH'}`;
      } else if (selectedAntPath) {
        antDetail.textContent = `Selected: ${selectedAntPath}`;
      } else {
        antDetail.textContent = 'Apache Ant not detected in PATH or selected.';
      }
      antDetail.title = antDetail.textContent;
    }

    // Check 2: Java (JDK)
    const effectiveJavaHome = (env && env.java_home) || selectedJavaHome || '';
    const javaOk = !!(env && env.java_installed && env.javac_installed) || !!effectiveJavaHome;
    if (javaBadge) {
      javaBadge.textContent = javaOk ? 'OK' : 'MISSING';
      javaBadge.className = `badge ${javaOk ? 'badge-green' : 'badge-red'}`;
    }
    if (javaDetail) {
      if (env && env.javac_installed) {
        const jv = (env.javac_version || env.java_version || '').split('\n')[0];
        javaDetail.textContent = `${jv || 'JDK Ready'} | ${effectiveJavaHome || 'PATH'}`;
      } else if (effectiveJavaHome) {
        javaDetail.textContent = `Selected JDK: ${effectiveJavaHome}`;
      } else {
        javaDetail.textContent = 'JDK / javac compiler not configured.';
      }
      javaDetail.title = javaDetail.textContent;
    }
    if (javaHomeEl) {
      javaHomeEl.textContent = effectiveJavaHome || '(Not configured)';
      javaHomeEl.title = javaHomeEl.textContent;
    }

    // Check 3: Run .bat Window Size (setting.json)
    const winW = (setting && setting.ps_window_width) || userSetting.ps_window_width;
    const winH = (setting && setting.ps_window_height) || userSetting.ps_window_height;
    const capOn = setting ? !!setting.auto_capture_clipboard : !!userSetting.auto_capture_clipboard;
    const winOk = winW >= 320 && winH >= 240;
    if (winBadge) {
      winBadge.textContent = winOk ? 'OK' : 'INVALID';
      winBadge.className = `badge ${winOk ? 'badge-green' : 'badge-yellow'}`;
    }
    if (winDetail) {
      winDetail.textContent = `${winW} x ${winH} px | Auto Capture: ${capOn ? 'ON' : 'OFF'}`;
      winDetail.title = winDetail.textContent;
    }

    if (propEl) {
      propEl.textContent = (tab && tab.propertiesFile) || '(Not found for this target)';
      propEl.title = propEl.textContent;
    }

    if (applyBtn) {
      applyBtn.disabled = !hasTarget;
    }

    if (summaryEl) {
      if (!hasTarget) {
        summaryEl.style.display = 'block';
        summaryEl.textContent =
          'Select a project directory and target package first before applying Auto Config.';
      } else if (antOk && javaOk && winOk && (!tab.targetPathValidation || tab.targetPathValidation.is_valid)) {
        summaryEl.style.display = 'none';
        summaryEl.textContent = '';
      } else {
        const missing = [];
        if (!antOk) missing.push('Apache Ant');
        if (!javaOk) missing.push('Java (JDK)');
        if (!winOk) missing.push('Window Size');
        const notes = [];
        if (missing.length > 0) {
          notes.push(
            `Warning: [${missing.join(', ')}] not ready. Use Config / Setting above to resolve, or apply path updates now.`
          );
        }
        if (tab.targetPathValidation && !tab.targetPathValidation.is_valid) {
          notes.push(`Target Path Warning: ${tab.targetPathValidation.summary}`);
        }
        summaryEl.style.display = 'block';
        summaryEl.textContent = notes.join('\n');
      }
    }
  } catch (err) {
    console.error('Auto config pre-check failed:', err);
    if (summaryEl) {
      summaryEl.style.display = 'block';
      summaryEl.textContent = `Check failed: ${err}`;
    }
  }
}

function setupBatSplitResizer() {
  const container = document.getElementById('bat-split-container');
  const resizer = document.getElementById('bat-split-resizer');
  if (!container || !resizer) return;

  let isDragging = false;

  resizer.addEventListener('mousedown', (e) => {
    e.preventDefault();
    isDragging = true;
    resizer.classList.add('dragging');
    document.body.style.cursor = 'col-resize';
  });

  document.addEventListener('mousemove', (e) => {
    if (!isDragging) return;
    const rect = container.getBoundingClientRect();
    if (!rect || rect.width <= 0) return;
    const rawPct = ((e.clientX - rect.left) / rect.width) * 100;
    applyBatSplitWidth(Math.round(rawPct * 10) / 10);
  });

  document.addEventListener('mouseup', () => {
    if (!isDragging) return;
    isDragging = false;
    resizer.classList.remove('dragging');
    document.body.style.cursor = '';
  });
}

// ==========================================================================
// 6. EVENT LISTENERS & INITIALIZATION
// ==========================================================================
function setupUIEvents() {
  // 0. Add New Tab Button
  document.getElementById('btn-add-tab').addEventListener('click', () => {
    addNewProjectTab('');
    const inputPathEl = document.getElementById('project-path-input');
    if (inputPathEl) inputPathEl.focus();
  });

  // 1. Browse Project Folder
  document.getElementById('btn-browse-folder').addEventListener('click', async () => {
    try {
      const folder = await invoke('pick_folder');
      if (folder) {
        await openOrSwitchProject(folder);
      }
    } catch (err) {
      console.error('Browse folder error:', err);
      showAlert('Browse Folder Error', escapeHtml(String(err)), '[X]');
    }
  });

  // 2. Custom History Dropdown (Toggle + Outside Click)
  const historyBtn = document.getElementById('btn-history-dropdown');
  const historyMenu = document.getElementById('history-dropdown-menu');
  const historyWrapper = document.getElementById('history-dropdown');

  if (historyBtn && historyMenu) {
    historyBtn.addEventListener('click', (e) => {
      e.stopPropagation();
      const isOpen = historyMenu.style.display !== 'none';
      historyMenu.style.display = isOpen ? 'none' : 'block';
    });

    document.addEventListener('click', (e) => {
      if (historyWrapper && !historyWrapper.contains(e.target)) {
        closeHistoryMenu();
      }
    });
  }

  // 3. Enter key or change in project path input (reuse existing tab or open new tab)
  const inputPathEl = document.getElementById('project-path-input');
  let lastSubmittedPathInput = '';
  const triggerPathScan = async (explicitTabId = null) => {
    const cleanPath = (inputPathEl.value || '').trim();
    if (!cleanPath) return;
    const ownerTab = (explicitTabId && getTabById(explicitTabId)) || getActiveTab();
    if (ownerTab && normalizeProjectPathKeyLocal(cleanPath) === getTabProjectKey(ownerTab)) {
      return;
    }
    lastSubmittedPathInput = cleanPath;
    try {
      await openOrSwitchProject(cleanPath);
    } catch (err) {
      console.error('Open project path failed:', err);
      showAlert('Open Project Error', escapeHtml(String(err)), '[X]');
    }
  };

  inputPathEl.addEventListener('focus', () => {
    inputPathEl.dataset.tabId = activeTabId;
  });
  inputPathEl.addEventListener('input', () => {
    lastSubmittedPathInput = '';
  });
  inputPathEl.addEventListener('keydown', async (e) => {
    if (e.key === 'Enter') {
      e.preventDefault();
      await triggerPathScan(activeTabId);
    }
  });
  inputPathEl.addEventListener('change', async () => {
    const curVal = (inputPathEl.value || '').trim();
    if (curVal && curVal === lastSubmittedPathInput) {
      return;
    }
    await triggerPathScan(inputPathEl.dataset.tabId || activeTabId);
  });

  // 3b. Target Build Path Configuration & Validation Controls
  const inputTargetDirEl = document.getElementById('target-build-path-input');
  let lastSubmittedTargetDir = '';
  const commitTargetPathInput = async (explicitTabId = null) => {
    const ownerTab = (explicitTabId && getTabById(explicitTabId)) || getActiveTab();
    if (!ownerTab || !ownerTab.projectPath) return;
    const rawVal = inputTargetDirEl ? (inputTargetDirEl.value || '').trim() : '';
    if (
      rawVal &&
      lastSubmittedTargetDir &&
      rawVal.toLowerCase() === lastSubmittedTargetDir.toLowerCase()
    ) {
      return;
    }
    const curEff = (ownerTab.effectiveTargetPath || '').trim();
    if (
      rawVal &&
      rawVal.replace(/\//g, '\\').toLowerCase() === curEff.replace(/\//g, '\\').toLowerCase()
    ) {
      return;
    }
    lastSubmittedTargetDir = rawVal;
    try {
      await applyTargetBuildPathForActiveTab(rawVal, {
        resetToDefault: rawVal === '',
        explicitTab: ownerTab,
      });
    } catch (err) {
      showAlert('Target Dir Error', escapeHtml(String(err)), '[X]');
    }
  };

  if (inputTargetDirEl) {
    inputTargetDirEl.addEventListener('focus', () => {
      inputTargetDirEl.dataset.tabId = activeTabId;
    });
    inputTargetDirEl.addEventListener('input', () => {
      lastSubmittedTargetDir = '';
    });
    inputTargetDirEl.addEventListener('keydown', async (e) => {
      if (e.key === 'Enter') {
        e.preventDefault();
        await commitTargetPathInput(activeTabId);
      }
    });
    inputTargetDirEl.addEventListener('change', async () => {
      await commitTargetPathInput(inputTargetDirEl.dataset.tabId || activeTabId);
    });
  }

  const pickCustomTargetDir = async () => {
    const tab = getActiveTab();
    if (!tab || !tab.projectPath) return null;
    try {
      const folder = await invoke('pick_folder');
      if (!folder) return null;
      lastSubmittedTargetDir = folder;
      return await applyTargetBuildPathForActiveTab(folder, { explicitTab: tab });
    } catch (err) {
      showAlert('Target Dir Error', escapeHtml(String(err)), '[X]');
      return null;
    }
  };

  document.getElementById('btn-browse-target-path').addEventListener('click', async () => {
    await pickCustomTargetDir();
  });

  document.getElementById('btn-reset-target-path').addEventListener('click', async () => {
    const tab = getActiveTab();
    if (!tab || !tab.projectPath) return;
    lastSubmittedTargetDir = '';
    try {
      await applyTargetBuildPathForActiveTab(null, { resetToDefault: true, explicitTab: tab });
    } catch (err) {
      showAlert('Target Dir Error', escapeHtml(String(err)), '[X]');
    }
  });

  document.getElementById('btn-fix-target-path-inline').addEventListener('click', async () => {
    const tab = getActiveTab();
    if (tab && tab.targetPathValidation) {
      openTargetPathWarningModal(tab.targetPathValidation);
    } else {
      await pickCustomTargetDir();
    }
  });

  document.getElementById('badge-target-path-status').addEventListener('click', () => {
    const tab = getActiveTab();
    if (tab && tab.targetPathValidation) {
      openTargetPathWarningModal(tab.targetPathValidation);
    }
  });

  document.getElementById('btn-close-modal-target-warning').addEventListener('click', closeTargetPathWarningModal);
  document.getElementById('modal-btn-close-target-warning').addEventListener('click', closeTargetPathWarningModal);
  document.getElementById('modal-btn-pick-shorter-target').addEventListener('click', async () => {
    const res = await pickCustomTargetDir();
    if (!res) return;
    if (res.validation && res.validation.is_valid) {
      closeTargetPathWarningModal();
      showAlert(
        'Target Dir Updated',
        `<b>Target Dir:</b> ${escapeHtml(res.effective_target_path)}<br><b>Status:</b> ${escapeHtml(res.validation.summary)}`,
        '[OK]'
      );
    } else if (res.validation) {
      populateTargetPathWarningModal(res.validation);
    }
  });

  // 4. Target selection change
  document.getElementById('select-ant-target').addEventListener('change', async (e) => {
    const tab = getActiveTab();
    if (!tab) return;
    tab.selectedTarget = e.target.value;
    if (tab.projectPath && tab.selectedTarget) {
      await Promise.all([
        inspectTargetBatFilesForTab(tab, tab.projectPath, tab.selectedTarget),
        refreshTargetPathValidationForTab(tab),
      ]);
    }
  });

  // 4b. Show Config Button (Right before Build -> opens .properties directly)
  document.getElementById('btn-show-config').addEventListener('click', async () => {
    await openConfigPropertiesDirect();
  });

  // 5. Run Build Button
  document.getElementById('btn-run-build').addEventListener('click', async () => {
    await executeBuildForActiveTab();
  });

  // 5b. Auto Config Button
  document.getElementById('btn-auto-config').addEventListener('click', async () => {
    await openAutoConfigModal();
  });

  // 6. Open Build Directory
  document.getElementById('btn-open-build-dir').addEventListener('click', async () => {
    const tab = getActiveTab();
    if (tab && tab.buildDir) {
      await invoke('open_path_in_explorer', { path: tab.buildDir });
    }
  });

  // 7. Dynamic Log File Buttons (no hardcoded log names)
  const openConfiguredLog = async (fileName, label) => {
    const tab = getActiveTab();
    if (!tab || !tab.projectPath || tab.projectPath.trim() === '') {
      showAlert(`${label} Not Found`, 'Please select a project directory first.', '[!]');
      return;
    }
    if (!fileName) {
      showAlert(
        `${label} Not Configured`,
        'No log filename was found in the project log4j properties.',
        '[!]'
      );
      return;
    }
    try {
      await invoke('open_file_log', {
        logFolderPath: tab.logFolder || null,
        projectPath: tab.projectPath,
        target: tab.selectedTarget || null,
        fileName: fileName,
        customTargetPath: tab.customTargetPath || null,
      });
    } catch (err) {
      const formatted = escapeHtml(String(err)).replace(/\n/g, '<br>');
      showAlert(`${fileName} Not Found`, formatted, '[!]');
    }
  };

  document.getElementById('btn-open-log').addEventListener('click', async () => {
    const tab = getActiveTab();
    await openConfiguredLog(tab ? tab.batchLogName : null, 'Batch Log');
  });

  document.getElementById('btn-open-app-log').addEventListener('click', async () => {
    const tab = getActiveTab();
    await openConfiguredLog(tab ? tab.appLogName : null, 'App Log');
  });

  // 8. Open Batch Directory in Explorer
  document.getElementById('btn-open-bat-dir').addEventListener('click', async () => {
    const tab = getActiveTab();
    if (tab && tab.batFolder) {
      await invoke('open_path_in_explorer', { path: tab.batFolder });
    }
  });

  // 9. Custom Batch Folder Input & Browse (bound safely to owning tabId)
  const inputBatFolder = document.getElementById('input-bat-folder');
  inputBatFolder.addEventListener('focus', () => {
    inputBatFolder.dataset.tabId = activeTabId;
  });
  inputBatFolder.addEventListener('keydown', async (e) => {
    if (e.key === 'Enter') {
      await scanCustomBatFolderForActiveTab(inputBatFolder.value, getActiveTab());
    }
  });
  inputBatFolder.addEventListener('change', async () => {
    const ownerTab = getTabById(inputBatFolder.dataset.tabId) || getActiveTab();
    await scanCustomBatFolderForActiveTab(inputBatFolder.value, ownerTab);
  });

  document.getElementById('btn-browse-bat-folder').addEventListener('click', async () => {
    try {
      const tab = getActiveTab();
      const folder = await invoke('pick_folder');
      if (folder && tab) {
        if (tab.id === activeTabId) {
          inputBatFolder.value = folder;
          inputBatFolder.dataset.tabId = tab.id;
        }
        await scanCustomBatFolderForActiveTab(folder, tab);
      }
    } catch (err) {
      console.error('Browse bat folder error:', err);
    }
  });

  // 10. Search / Filter Batch Files & Preview Actions
  document.getElementById('input-bat-filter').addEventListener('input', () => {
    applyBatFilterAndRender();
  });

  document.getElementById('btn-toggle-bat-theme').addEventListener('click', () => {
    batViewerDarkTheme = !batViewerDarkTheme;
    try {
      localStorage.setItem('bh_bat_viewer_theme', batViewerDarkTheme ? 'dark' : 'light');
    } catch (_) {}
    applyBatThemeUI();
  });

  document.getElementById('btn-open-selected-bat').addEventListener('click', async () => {
    const tab = getActiveTab();
    if (tab && tab.selectedBatPath) {
      await openBatInTextEditor(tab.selectedBatPath);
    }
  });

  document.getElementById('btn-run-selected-bat').addEventListener('click', async () => {
    const tab = getActiveTab();
    if (tab && tab.selectedBatPath) {
      await runBatScript(tab.selectedBatPath);
    }
  });

  document.addEventListener('click', (e) => {
    const menu = document.getElementById('bat-context-menu');
    if (menu && menu.style.display !== 'none' && !menu.contains(e.target)) {
      closeBatContextMenu();
    }
    const grpMenu = document.getElementById('group-context-menu');
    if (grpMenu && grpMenu.style.display !== 'none' && !grpMenu.contains(e.target)) {
      closeGroupContextMenu();
    }
  });

  // Track whether the BAT file list zone is active for keyboard shortcuts (Ctrl + A)
  const updateBatActiveZone = (target) => {
    if (!target || typeof target.closest !== 'function') return;
    if (target.closest('#bat-table-container') || target.closest('#bat-groups-bar')) {
      batListActive = true;
    } else if (
      !target.closest('#bat-context-menu') &&
      !target.closest('#group-context-menu')
    ) {
      batListActive = false;
    }
  };
  document.addEventListener('mousedown', (e) => updateBatActiveZone(e.target));
  document.addEventListener('focusin', (e) => updateBatActiveZone(e.target));

  document.addEventListener('keydown', (e) => {
    if (e.isComposing) return;
    const isCtrlA =
      (e.ctrlKey || e.metaKey) &&
      !e.altKey &&
      !e.shiftKey &&
      e.key &&
      e.key.toLowerCase() === 'a';
    if (!isCtrlA) return;
    if (isAnyModalVisible()) return;

    const ae = document.activeElement;
    if (ae) {
      const tag = (ae.tagName || '').toUpperCase();
      if (tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT' || ae.isContentEditable) {
        return;
      }
      if (typeof ae.closest === 'function') {
        if (ae.closest('#bat-preview-pane') || ae.closest('#cmd-panel-box')) {
          return;
        }
      }
    }

    const tableContainer = document.getElementById('bat-table-container');
    const inBatContainer = !!(tableContainer && ae && tableContainer.contains(ae));
    if (!batListActive && !inBatContainer) {
      return;
    }

    const activeTab = getActiveTab();
    if (!activeTab) return;
    closeBatContextMenu();
    closeGroupContextMenu();
    if (selectAllVisibleBatFiles(activeTab)) {
      e.preventDefault();
      e.stopPropagation();
    }
  });

  setupBatSplitResizer();

  // 10b. CMD Drawer Controls (Toggle / Expand / Hide)
  const toggleCmdDrawer = () => {
    const tab = getActiveTab();
    if (!tab) return;
    tab.cmdPanelOpen = !tab.cmdPanelOpen;
    syncUIToActiveTab();
  };
  document.getElementById('btn-toggle-cmd').addEventListener('click', toggleCmdDrawer);
  document.getElementById('badge-build-status').addEventListener('click', toggleCmdDrawer);

  document.getElementById('btn-hide-cmd').addEventListener('click', () => {
    const tab = getActiveTab();
    if (!tab) return;
    tab.cmdPanelOpen = false;
    syncUIToActiveTab();
  });

  document.getElementById('btn-expand-cmd').addEventListener('click', () => {
    const tab = getActiveTab();
    if (!tab) return;
    tab.cmdPanelExpanded = !tab.cmdPanelExpanded;
    syncUIToActiveTab();
  });

  // 10c. Clear All Log (Displayed console logs & optional disk target .log truncation with confirmation)
  const clearAllLogsForTab = async (tab, includeDiskFiles = false) => {
    if (!tab) return;
    tab.consoleLogs = '';
    if (tab.id === activeTabId) {
      const consoleEl = document.getElementById('build-console-output');
      if (consoleEl) {
        consoleEl.textContent = '';
      }
      const clearAllBtn = document.getElementById('btn-clear-all-log');
      if (clearAllBtn) {
        clearAllBtn.disabled = !(tab.projectPath && tab.projectPath.trim() !== '');
      }
    }

    if (!includeDiskFiles || !tab.projectPath || tab.projectPath.trim() === '') {
      setTabFooterStatus(tab, 'Logs cleared', formatCurrentDateTime(), 'ok');
      return;
    }

    try {
      const clearRes = await invoke('clear_target_logs', {
        projectPath: tab.projectPath,
        target: tab.selectedTarget || null,
        logFolderPath: tab.logFolder || null,
        batchLogName: tab.batchLogName || null,
        appLogName: tab.appLogName || null,
        customTargetPath: tab.customTargetPath || null,
      });
      const clearedCount = (clearRes && clearRes.cleared_files && clearRes.cleared_files.length) || 0;
      const failedList = (clearRes && clearRes.failed_files) || [];
      if (failedList.length > 0) {
        setTabFooterStatus(tab, `Cleared ${clearedCount} log(s), ${failedList.length} locked`, formatCurrentDateTime(), 'err');
        showAlert(
          'Clear Log Partial Warning',
          `Console log cleared and truncated ${clearedCount} target .log file(s), but ${failedList.length} file(s) could not be modified:<br><br>${failedList.map(escapeHtml).join('<br>')}`,
          '[!]'
        );
      } else {
        setTabFooterStatus(tab, `Cleared all logs (${clearedCount} file(s))`, formatCurrentDateTime(), 'ok');
      }
    } catch (err) {
      setTabFooterStatus(tab, 'Clear log failed', formatCurrentDateTime(), 'err');
      showAlert('Clear Log Error', `Failed to clear target log files:<br>${escapeHtml(String(err))}`, '[X]');
    }
  };

  let pendingClearLogTabId = '';
  const closeClearLogConfirmModal = () => {
    const el = document.getElementById('modal-confirm-clear-log');
    if (el) el.style.display = 'none';
    pendingClearLogTabId = '';
  };

  document.getElementById('btn-clear-cmd-log').addEventListener('click', () => {
    const tab = getActiveTab();
    if (!tab) return;
    clearAllLogsForTab(tab, false);
  });

  document.getElementById('btn-clear-all-log').addEventListener('click', () => {
    const tab = getActiveTab();
    if (!tab) return;
    pendingClearLogTabId = tab.id;
    const msgEl = document.getElementById('clear-log-confirm-msg');
    if (msgEl) {
      const targetNote = tab.logFolder
        ? escapeHtml(tab.logFolder)
        : tab.effectiveTargetPath
          ? `${escapeHtml(tab.effectiveTargetPath)}\\log`
          : tab.projectPath
            ? `${escapeHtml(tab.projectPath)}\\target\\log`
            : '(No project selected)';
      msgEl.innerHTML =
        `Clear all displayed CMD logs and truncate all <b>.log</b> files inside:<br>` +
        `<span class="text-mono">${targetNote}</span><br><br>` +
        `This leaves all <b>.bat</b> files, project history, and saved settings untouched.`;
    }
    openSingleModal('modal-confirm-clear-log');
  });

  document.getElementById('btn-close-modal-clear-log').addEventListener('click', closeClearLogConfirmModal);
  document.getElementById('modal-btn-cancel-clear-log').addEventListener('click', closeClearLogConfirmModal);
  document.getElementById('modal-btn-confirm-clear-log').addEventListener('click', async () => {
    const tab = (pendingClearLogTabId && getTabById(pendingClearLogTabId)) || getActiveTab();
    closeClearLogConfirmModal();
    await clearAllLogsForTab(tab, true);
  });

  // 11. Footer Click -> Open Single Non-Overlapping Modal
  document.getElementById('footer-ant-btn').addEventListener('click', () => {
    openSingleModal('modal-ant');
  });

  document.getElementById('footer-java-btn').addEventListener('click', () => {
    openSingleModal('modal-java');
  });

  // Database Modal Helpers & Events
  const updateDbPasswordDisplay = (pwd) => {
    const pwdEl = document.getElementById('modal-db-password');
    const toggleBtn = document.getElementById('modal-btn-toggle-db-pwd');
    if (!pwdEl) return;
    if (!pwd) {
      pwdEl.textContent = '-';
      if (toggleBtn) toggleBtn.style.display = 'none';
      return;
    }
    if (toggleBtn) {
      toggleBtn.style.display = 'inline-block';
      toggleBtn.textContent = dbPasswordVisible ? 'Hide' : 'Show';
    }
    pwdEl.textContent = dbPasswordVisible ? pwd : '*'.repeat(Math.min(Math.max(pwd.length, 6), 14));
  };

  const renderDatabaseModal = (info, tab) => {
    const statusEl = document.getElementById('modal-db-status');
    const driverEl = document.getElementById('modal-db-driver');
    const serverEl = document.getElementById('modal-db-server');
    const portEl = document.getElementById('modal-db-port');
    const nameEl = document.getElementById('modal-db-name');
    const userEl = document.getElementById('modal-db-user');
    const poolEl = document.getElementById('modal-db-pool');
    const urlEl = document.getElementById('modal-db-url');
    const propEl = document.getElementById('modal-db-prop-file');
    const warnEl = document.getElementById('modal-db-warning');
    const testBtn = document.getElementById('modal-btn-test-db');
    const openPropBtn = document.getElementById('modal-btn-open-db-prop');

    const hasConfig = !!(info && info.found);
    if (statusEl) {
      if (!tab || !tab.projectPath || !tab.selectedTarget) {
        statusEl.innerHTML = '<span class="badge badge-gray">NO PROJECT SELECTED</span>';
      } else if (hasConfig) {
        statusEl.innerHTML = `<span class="badge badge-green">CONFIGURED (${escapeHtml(info.db_type || 'DB')})</span>`;
      } else {
        statusEl.innerHTML = '<span class="badge badge-yellow">NOT FOUND IN PROPERTIES</span>';
      }
    }

    if (driverEl) {
      driverEl.textContent = info && info.driver
        ? `${info.db_type || 'DB'} | ${info.driver}`
        : (info && info.db_type) || '-';
    }
    if (serverEl) {
      serverEl.textContent = (info && (info.effective_server || info.host)) || '-';
    }
    if (portEl) {
      portEl.textContent = (info && info.port_or_instance) || '-';
    }
    if (nameEl) {
      nameEl.textContent = (info && info.database) || '-';
    }
    if (userEl) {
      userEl.textContent = (info && info.user) || '-';
    }
    updateDbPasswordDisplay(info ? info.password : null);

    if (poolEl) {
      const initP = info && info.init_pool ? info.init_pool : '-';
      const maxP = info && info.max_pool ? info.max_pool : '-';
      poolEl.textContent = info && (info.init_pool || info.max_pool)
        ? `init: ${initP} / max: ${maxP}`
        : '-';
    }
    if (urlEl) {
      urlEl.textContent = (info && info.jdbc_url) || '-';
      urlEl.title = urlEl.textContent;
    }
    if (propEl) {
      propEl.textContent = (info && info.properties_file) || '-';
      propEl.title = propEl.textContent;
    }

    if (warnEl) {
      if (info && info.warning_message) {
        warnEl.style.display = 'block';
        warnEl.textContent = info.warning_message;
      } else {
        warnEl.style.display = 'none';
        warnEl.textContent = '';
      }
    }

    if (testBtn) {
      testBtn.disabled = !(tab && tab.projectPath && tab.selectedTarget && hasConfig);
    }
    if (openPropBtn) {
      openPropBtn.disabled = !(info && info.properties_file);
    }
  };

  const refreshAndRenderDbModal = async (clearResult = true) => {
    const tab = getActiveTab();
    dbPasswordVisible = false;
    const resultEl = document.getElementById('modal-db-test-result');
    if (clearResult && resultEl) {
      resultEl.style.display = 'none';
      resultEl.innerHTML = '';
    }

    renderDatabaseModal(tab ? tab.dbInfo : null, tab);
    if (tab && tab.projectPath && tab.selectedTarget) {
      const freshInfo = await refreshDatabaseInfoForTab(tab);
      if (freshInfo) {
        renderDatabaseModal(freshInfo, tab);
      }
    }
  };

  const closeDbModal = () => {
    document.getElementById('modal-db').style.display = 'none';
  };

  document.getElementById('footer-db-btn').addEventListener('click', async () => {
    openSingleModal('modal-db');
    await refreshAndRenderDbModal(true);
  });

  document.getElementById('btn-close-modal-db').addEventListener('click', closeDbModal);
  document.getElementById('modal-btn-close-db').addEventListener('click', closeDbModal);

  document.getElementById('modal-btn-refresh-db').addEventListener('click', async () => {
    await refreshAndRenderDbModal(false);
  });

  document.getElementById('modal-btn-toggle-db-pwd').addEventListener('click', () => {
    dbPasswordVisible = !dbPasswordVisible;
    const tab = getActiveTab();
    updateDbPasswordDisplay(tab && tab.dbInfo ? tab.dbInfo.password : null);
  });

  document.getElementById('modal-btn-open-db-prop').addEventListener('click', async () => {
    const tab = getActiveTab();
    const propPath = tab && tab.dbInfo && tab.dbInfo.properties_file;
    if (!propPath) return;
    try {
      await invoke('open_file_default', { path: propPath });
    } catch (err) {
      showAlert('Open Config Error', `Failed to open properties file: ${escapeHtml(err)}`, '[X]');
    }
  });

  document.getElementById('modal-btn-test-db').addEventListener('click', async (e) => {
    const tab = getActiveTab();
    if (!tab || !tab.projectPath || !tab.selectedTarget) return;

    const btn = e.currentTarget;
    const resultEl = document.getElementById('modal-db-test-result');
    try {
      btn.disabled = true;
      btn.textContent = 'Testing...';
      if (resultEl) {
        resultEl.style.display = 'block';
        resultEl.innerHTML = '<span class="badge badge-yellow">TESTING...</span> Connecting to database...';
      }

      const freshInfo = await refreshDatabaseInfoForTab(tab);
      if (freshInfo) {
        renderDatabaseModal(freshInfo, tab);
      }

      const res = await invoke('test_database_connection', {
        projectPath: tab.projectPath,
        target: tab.selectedTarget,
      });

      if (resultEl) {
        const badge = res.success
          ? '<span class="badge badge-green">CONNECTED</span>'
          : '<span class="badge badge-red">FAILED</span>';
        const verLine = res.server_version
          ? `<br><b>Version:</b> ${escapeHtml(res.server_version)}`
          : '';
        resultEl.style.display = 'block';
        resultEl.innerHTML =
          `${badge} <b>${escapeHtml(res.db_type)}</b> | ${escapeHtml(res.target_server || '-')} | DB: <b>${escapeHtml(res.database || '-')}</b> (${res.duration_ms} ms)` +
          verLine +
          `<br>${escapeHtml(res.message || '')}`;
      }
    } catch (err) {
      if (resultEl) {
        resultEl.style.display = 'block';
        resultEl.innerHTML = `<span class="badge badge-red">ERROR</span> ${escapeHtml(String(err))}`;
      }
    } finally {
      btn.disabled = false;
      btn.textContent = 'Test Connection';
    }
  });

  document.getElementById('footer-config-btn').addEventListener('click', () => {
    populateSettingsModal();
    openSingleModal('modal-settings');
  });

  // Settings Modal Handlers
  const closeSettingsModal = () => {
    document.getElementById('modal-settings').style.display = 'none';
  };
  document.getElementById('btn-close-modal-settings').addEventListener('click', closeSettingsModal);
  document.getElementById('modal-btn-close-settings').addEventListener('click', closeSettingsModal);

  document.getElementById('setting-auto-capture').addEventListener('change', () => {
    updateScreenshotToggleBadge();
  });

  document.getElementById('setting-auto-save-image').addEventListener('change', () => {
    updateScreenshotToggleBadge();
  });

  document.getElementById('modal-btn-browse-screenshot-dir').addEventListener('click', async () => {
    try {
      const folder = await invoke('pick_folder');
      if (folder) {
        const dirEl = document.getElementById('setting-screenshot-dir');
        if (dirEl) dirEl.value = folder;
      }
    } catch (err) {
      console.error('Browse screenshot folder failed:', err);
    }
  });

  document.getElementById('modal-btn-default-screenshot-dir').addEventListener('click', () => {
    const dirEl = document.getElementById('setting-screenshot-dir');
    if (dirEl) {
      dirEl.value = defaultScreenshotRootDir || '';
    }
    updateScreenshotToggleBadge();
  });

  document.getElementById('setting-screenshot-dir').addEventListener('input', () => {
    updateScreenshotToggleBadge();
  });

  document.getElementById('modal-btn-open-screenshot-dir').addEventListener('click', async () => {
    const dirEl = document.getElementById('setting-screenshot-dir');
    const customRaw = dirEl ? (dirEl.value || '').trim() : '';
    const isSameAsDefault =
      defaultScreenshotRootDir &&
      customRaw.replace(/[\\/]+$/, '').toLowerCase() ===
        defaultScreenshotRootDir.replace(/[\\/]+$/, '').toLowerCase();
    try {
      await invoke('open_screenshot_directory', {
        customDir: !customRaw || isSameAsDefault ? null : customRaw,
      });
    } catch (err) {
      showAlert('Open Folder Error', `Failed to open screenshot directory:<br>${escapeHtml(String(err))}`, '[X]');
    }
  });

  document.getElementById('modal-btn-reset-settings').addEventListener('click', () => {
    const widthEl = document.getElementById('setting-ps-width');
    const heightEl = document.getElementById('setting-ps-height');
    const capEl = document.getElementById('setting-auto-capture');
    const saveImgEl = document.getElementById('setting-auto-save-image');
    const dirEl = document.getElementById('setting-screenshot-dir');
    if (widthEl) widthEl.value = 1000;
    if (heightEl) heightEl.value = 650;
    if (capEl) capEl.checked = true;
    if (saveImgEl) saveImgEl.checked = false;
    if (dirEl) dirEl.value = defaultScreenshotRootDir || '';
    updateScreenshotToggleBadge();
  });

  document.getElementById('modal-btn-pick-size').addEventListener('click', async () => {
    const widthEl = document.getElementById('setting-ps-width');
    const heightEl = document.getElementById('setting-ps-height');
    const width = parseInt(widthEl && widthEl.value, 10) || userSetting.ps_window_width || 1000;
    const height = parseInt(heightEl && heightEl.value, 10) || userSetting.ps_window_height || 650;
    try {
      await invoke('open_cmd_size_picker', { width, height });
    } catch (err) {
      showAlert('Size Picker Error', `Failed to open simulated CMD window: ${escapeHtml(err)}`, '[X]');
    }
  });

  document.getElementById('modal-btn-save-settings').addEventListener('click', async () => {
    const widthVal = parseInt(document.getElementById('setting-ps-width').value, 10) || 1000;
    const heightVal = parseInt(document.getElementById('setting-ps-height').value, 10) || 650;
    const autoCapVal = !!document.getElementById('setting-auto-capture').checked;
    const autoSaveImgVal = !!document.getElementById('setting-auto-save-image').checked;
    const rawDirVal = (document.getElementById('setting-screenshot-dir').value || '').trim();
    const isSameAsDefault =
      defaultScreenshotRootDir &&
      rawDirVal.replace(/[\\/]+$/, '').toLowerCase() ===
        defaultScreenshotRootDir.replace(/[\\/]+$/, '').toLowerCase();
    const saveDirVal = !rawDirVal || isSameAsDefault ? null : rawDirVal;

    try {
      const saved = await invoke('save_user_setting', {
        setting: {
          ps_window_width: widthVal,
          ps_window_height: heightVal,
          auto_capture_clipboard: autoCapVal,
          auto_save_image: autoSaveImgVal,
          screenshot_save_dir: saveDirVal,
        },
      });
      userSetting = saved;
      updateFooterConfigVal();
      closeSettingsModal();
      const effectivePath = saved.screenshot_save_dir || defaultScreenshotRootDir || '(Default)';
      showAlert(
        'Settings Saved',
        `<b>Window Size:</b> ${saved.ps_window_width} x ${saved.ps_window_height} px<br><b>Auto Capture:</b> ${saved.auto_capture_clipboard ? 'ON' : 'OFF'}<br><b>Auto Save Image:</b> ${saved.auto_save_image ? 'ON' : 'OFF'}<br><b>Image Path:</b> ${escapeHtml(effectivePath)}`,
        '[OK]'
      );
    } catch (err) {
      showAlert('Save Error', `Failed to save setting.json:<br>${escapeHtml(String(err))}`, '[X]');
    }
  });

  // Modal Close buttons (Ant & Java)
  const closeAntModal = () => {
    document.getElementById('modal-ant').style.display = 'none';
  };
  document.getElementById('btn-close-modal-ant').addEventListener('click', closeAntModal);
  document.getElementById('modal-btn-close-ant').addEventListener('click', closeAntModal);

  const closeJavaModal = () => {
    document.getElementById('modal-java').style.display = 'none';
  };
  document.getElementById('btn-close-modal-java').addEventListener('click', closeJavaModal);
  document.getElementById('modal-btn-close-java').addEventListener('click', closeJavaModal);

  // Auto Config Modal Handlers (1. Close Auto Config first, 2. Open target modal)
  const closeAutoConfigModal = () => {
    document.getElementById('modal-auto-config').style.display = 'none';
  };
  document.getElementById('btn-close-modal-autoconfig').addEventListener('click', closeAutoConfigModal);
  document.getElementById('modal-btn-close-autoconfig').addEventListener('click', closeAutoConfigModal);

  document.getElementById('autoconfig-btn-fix-ant').addEventListener('click', () => {
    closeAutoConfigModal();
    openSingleModal('modal-ant');
  });
  document.getElementById('autoconfig-btn-fix-java').addEventListener('click', () => {
    closeAutoConfigModal();
    openSingleModal('modal-java');
  });
  document.getElementById('autoconfig-btn-fix-window').addEventListener('click', () => {
    closeAutoConfigModal();
    populateSettingsModal();
    openSingleModal('modal-settings');
  });

  document.getElementById('modal-btn-recheck-autoconfig').addEventListener('click', async () => {
    await refreshAutoConfigChecks();
  });

  document.getElementById('modal-btn-apply-autoconfig').addEventListener('click', async (e) => {
    const tab = getActiveTab();
    if (!tab || !tab.projectPath || !tab.selectedTarget) {
      showAlert('Auto Config Error', 'Please select a project and target package first.', '[!]');
      return;
    }

    const btn = e.currentTarget;
    try {
      btn.disabled = true;
      btn.textContent = 'Applying...';

      const jh = selectedJavaHome || (envData && envData.java_home) || null;
      const res = await invoke('run_auto_config', {
        projectPath: tab.projectPath,
        target: tab.selectedTarget,
        javaHome: jh,
        customTargetPath: tab.customTargetPath || null,
      });

      await Promise.all([
        inspectTargetBatFilesForTab(tab, tab.projectPath, tab.selectedTarget),
        refreshTargetPathValidationForTab(tab),
      ]);
      closeAutoConfigModal();
      const formatted = escapeHtml(res.message || 'Auto Config complete.').replace(/\n/g, '<br>');
      showAlert('Auto Config Complete', formatted, '[OK]');
    } catch (err) {
      showAlert('Auto Config Error', escapeHtml(String(err)), '[X]');
    } finally {
      btn.disabled = false;
      btn.textContent = 'Apply Auto Config';
    }
  });

  // Group Run Modal Close Handlers
  const closeGroupRunModal = () => {
    if (activeGroupRunLocked) return;
    document.getElementById('modal-group-run').style.display = 'none';
  };
  document.getElementById('btn-close-modal-group-run').addEventListener('click', closeGroupRunModal);
  document.getElementById('modal-btn-close-group-run').addEventListener('click', closeGroupRunModal);
  document.getElementById('modal-btn-open-group-image').addEventListener('click', async () => {
    const activeStep = groupRunSteps[groupRunSelectedIdx] || null;
    if (!activeStep || !activeStep.screenshot_path) return;
    try {
      await invoke('open_file_default', { path: activeStep.screenshot_path });
    } catch (err) {
      showAlert('Open Image Error', `Failed to open screenshot:<br>${escapeHtml(String(err))}`, '[X]');
    }
  });

  // Add Batch Group Modal Handlers
  const closeAddGroupModal = () => {
    document.getElementById('modal-add-group').style.display = 'none';
    addGroupPendingFiles = [];
    addGroupPendingTabId = '';
  };

  const submitAddGroupModal = async () => {
    const tab = (addGroupPendingTabId && getTabById(addGroupPendingTabId)) || getActiveTab();
    const errorEl = document.getElementById('add-group-error');
    const radioNew = document.getElementById('add-group-mode-new');
    const newNameEl = document.getElementById('add-group-new-name');
    const existingSelEl = document.getElementById('add-group-existing-select');
    const saveBtn = document.getElementById('modal-btn-save-add-group');

    const showErr = (msg) => {
      if (errorEl) {
        errorEl.style.display = 'block';
        errorEl.textContent = msg;
      }
    };

    if (!tab || !tab.projectPath) {
      showErr('Please select a project directory first.');
      return;
    }
    const filePaths = (addGroupPendingFiles || []).map((f) => f.path).filter(Boolean);
    if (filePaths.length === 0) {
      showErr('No valid .bat files selected.');
      return;
    }

    const isNewMode = radioNew ? !!radioNew.checked : true;
    try {
      if (saveBtn) saveBtn.disabled = true;
      let updatedGroups = [];
      if (isNewMode) {
        const groupName = (newNameEl && newNameEl.value ? newNameEl.value : '').trim();
        if (!groupName) {
          showErr('Please enter a group name.');
          if (newNameEl) newNameEl.focus();
          return;
        }
        updatedGroups = await invoke('create_batch_group', {
          projectPath: tab.projectPath,
          name: groupName,
          files: filePaths,
        });
      } else {
        const groupId = existingSelEl ? existingSelEl.value : '';
        if (!groupId) {
          showErr('Please select an existing group.');
          return;
        }
        updatedGroups = await invoke('add_files_to_batch_group', {
          projectPath: tab.projectPath,
          groupId,
          files: filePaths,
        });
      }

      tab.groups = Array.isArray(updatedGroups) ? updatedGroups : [];
      closeAddGroupModal();
      if (tab.id === activeTabId) {
        renderGroupsBar(tab);
        applyBatFilterAndRender(tab);
      }
    } catch (err) {
      showErr(String(err));
    } finally {
      if (saveBtn) saveBtn.disabled = false;
    }
  };

  document.getElementById('btn-close-modal-add-group').addEventListener('click', closeAddGroupModal);
  document.getElementById('modal-btn-cancel-add-group').addEventListener('click', closeAddGroupModal);
  document.getElementById('modal-btn-save-add-group').addEventListener('click', submitAddGroupModal);

  document.getElementById('add-group-new-name').addEventListener('focus', () => {
    const radioNew = document.getElementById('add-group-mode-new');
    if (radioNew) radioNew.checked = true;
  });
  document.getElementById('add-group-existing-select').addEventListener('focus', () => {
    const radioExisting = document.getElementById('add-group-mode-existing');
    if (radioExisting && !radioExisting.disabled) radioExisting.checked = true;
  });
  document.getElementById('add-group-new-name').addEventListener('keydown', (e) => {
    if (e.key === 'Enter') {
      e.preventDefault();
      submitAddGroupModal();
    }
  });

  // Browse custom JDK folder in Modal (mark manual pick)
  // Browse custom Ant folder in Modal
  document.getElementById('modal-btn-browse-ant').addEventListener('click', async () => {
    try {
      const folder = await invoke('pick_folder');
      if (folder) {
        selectedAntPath = `${folder}\\bin\\ant.bat`;
        selectedAntRootOrZip = folder;
        const descEl = document.getElementById('modal-ant-choice-desc');
        if (descEl) descEl.textContent = `Custom Path: ${folder}`;
      }
    } catch (err) {
      console.error(err);
    }
  });

  // Browse custom JDK folder in Modal
  document.getElementById('modal-btn-browse-jdk').addEventListener('click', async () => {
    try {
      const folder = await invoke('pick_folder');
      if (folder) {
        userManuallyPickedJdk = true;
        selectedJavaHome = folder;
        const descEl = document.getElementById('modal-jdk-choice-desc');
        if (descEl) descEl.textContent = `Custom Path: ${folder}`;
      }
    } catch (err) {
      console.error(err);
    }
  });

  // Global ESC key handler: close only the topmost active menu or modal (matches X button)
  document.addEventListener('keydown', (e) => {
    if (e.key !== 'Escape' || e.isComposing) return;

    const ctxMenu = document.getElementById('bat-context-menu');
    if (ctxMenu && ctxMenu.style.display !== 'none') {
      closeBatContextMenu();
      e.preventDefault();
      e.stopPropagation();
      return;
    }

    const grpCtxMenu = document.getElementById('group-context-menu');
    if (grpCtxMenu && grpCtxMenu.style.display !== 'none') {
      closeGroupContextMenu();
      e.preventDefault();
      e.stopPropagation();
      return;
    }

    const histMenu = document.getElementById('history-dropdown-menu');
    if (histMenu && histMenu.style.display !== 'none') {
      closeHistoryMenu();
      e.preventDefault();
      e.stopPropagation();
      return;
    }

    if (isModalVisible('alert-modal')) {
      closeAlert();
      e.preventDefault();
      e.stopPropagation();
      return;
    }

    if (isModalVisible('modal-target-path-warning')) {
      closeTargetPathWarningModal();
      e.preventDefault();
      e.stopPropagation();
      return;
    }

    if (isModalVisible('modal-confirm-clear-log')) {
      closeClearLogConfirmModal();
      e.preventDefault();
      e.stopPropagation();
      return;
    }

    if (isModalVisible('modal-add-group')) {
      closeAddGroupModal();
      e.preventDefault();
      e.stopPropagation();
      return;
    }

    if (isModalVisible('modal-group-run')) {
      if (!activeGroupRunLocked) {
        closeGroupRunModal();
      }
      e.preventDefault();
      e.stopPropagation();
      return;
    }

    if (isModalVisible('modal-settings')) {
      closeSettingsModal();
      e.preventDefault();
      e.stopPropagation();
      return;
    }

    if (isModalVisible('modal-java')) {
      closeJavaModal();
      e.preventDefault();
      e.stopPropagation();
      return;
    }

    if (isModalVisible('modal-db')) {
      closeDbModal();
      e.preventDefault();
      e.stopPropagation();
      return;
    }

    if (isModalVisible('modal-ant')) {
      closeAntModal();
      e.preventDefault();
      e.stopPropagation();
      return;
    }

    if (isModalVisible('modal-auto-config')) {
      closeAutoConfigModal();
      e.preventDefault();
      e.stopPropagation();
    }
  });

  // Quick Set Environment: Ant
  document.getElementById('modal-btn-set-ant-env').addEventListener('click', async (e) => {
    const btn = e.currentTarget;
    try {
      btn.disabled = true;
      btn.textContent = 'Setting...';

      let targetPath = selectedAntRootOrZip || selectedAntPath;
      if (targetPath && targetPath.toLowerCase().endsWith('.zip')) {
        const extRes = await invoke('extract_ant', { zipPath: targetPath });
        if (!extRes.success) {
          showAlert('Extract Failed', extRes.message, '[X]');
          return;
        }
        targetPath = extRes.extracted_dir || extRes.executable_path;
        await loadAntOptions();
      }

      if (!targetPath && envData && envData.ant_home) {
        targetPath = envData.ant_home;
      }

      if (!targetPath) {
        showAlert('Ant Path Required', 'Please select or browse an Ant directory first.', '[!]');
        return;
      }

      const updatedEnv = await invoke('set_ant_env_quick', { antPath: targetPath });
      renderEnvironmentUI(updatedEnv);
      showAlert(
        'Environment Updated',
        `<b>ANT_HOME:</b> ${updatedEnv.ant_home || 'Set'}<br><b>where ant:</b> ${updatedEnv.ant_where_path || 'Configured'}`,
        '[OK]'
      );
    } catch (err) {
      showAlert('Set Environment Error', `${err}`, '[X]');
    } finally {
      btn.disabled = false;
      btn.textContent = 'Set Environment';
    }
  });

  // Quick Set Environment: Java
  document.getElementById('modal-btn-set-java-env').addEventListener('click', async (e) => {
    const btn = e.currentTarget;
    try {
      btn.disabled = true;
      btn.textContent = 'Setting...';

      const targetPath = selectedJavaHome || (envData && envData.java_home);
      if (!targetPath) {
        showAlert('JDK Path Required', 'Please select or browse a JDK directory first.', '[!]');
        return;
      }

      const updatedEnv = await invoke('set_java_env_quick', { javaPath: targetPath });
      userManuallyPickedJdk = false;
      renderEnvironmentUI(updatedEnv);
      const curTab = getActiveTab();
      if (curTab && curTab.projectPath) {
        await refreshTargetPathValidationForTab(curTab);
      }
      showAlert(
        'Environment Updated',
        `<b>JAVA_HOME:</b> ${updatedEnv.java_home || 'Set'}<br><b>where java:</b> ${updatedEnv.java_where_path || 'Configured'}`,
        '[OK]'
      );
    } catch (err) {
      showAlert('Set Environment Error', `${err}`, '[X]');
    } finally {
      btn.disabled = false;
      btn.textContent = 'Set Environment';
    }
  });

  // Modal Tests
  document.getElementById('modal-btn-test-ant').addEventListener('click', async () => {
    const testPath = selectedAntPath || (envData && envData.ant_where_path) || 'ant';
    try {
      const res = await invoke('test_ant', { executablePath: testPath });
      showAlert(
        'Ant Test Result',
        `<b>Success:</b> ${res.success ? 'Yes' : 'No'}<br><b>Version:</b> ${res.version_output || 'N/A'}<br><b>Exit Code:</b> ${res.exit_code}`,
        res.success ? '[OK]' : '[X]'
      );
    } catch (err) {
      showAlert('Ant Test Error', `${err}`, '[X]');
    }
  });

  document.getElementById('modal-btn-test-java').addEventListener('click', async () => {
    const testPath = selectedJavaHome ? `${selectedJavaHome}\\bin\\java.exe` : 'java';
    try {
      const res = await invoke('test_java', { executablePath: testPath });
      showAlert(
        'Java Test Result',
        `<b>Success:</b> ${res.success ? 'Yes' : 'No'}<br><b>Version:</b> ${res.version_output || 'N/A'}<br><b>Exit Code:</b> ${res.exit_code}`,
        res.success ? '[OK]' : '[X]'
      );
    } catch (err) {
      showAlert('Java Test Error', `${err}`, '[X]');
    }
  });

  // Modal Extract Ant
  document.getElementById('modal-btn-extract-ant').addEventListener('click', async (e) => {
    const zipPath = e.target.getAttribute('data-zip');
    if (!zipPath) return;
    try {
      e.target.disabled = true;
      e.target.textContent = 'Extracting...';
      const res = await invoke('extract_ant', { zipPath });
      showAlert('Extract Ant', res.message, res.success ? '[OK]' : '[X]');
      await loadAntOptions();
    } catch (err) {
      showAlert('Extract Error', `${err}`, '[X]');
    } finally {
      e.target.disabled = false;
      e.target.textContent = 'Extract ZIP';
    }
  });

  // Alert Modal
  document.getElementById('btn-close-alert').addEventListener('click', closeAlert);
  document.getElementById('btn-ok-alert').addEventListener('click', closeAlert);
}

// App Initialization
async function init() {
  setupUIEvents();

  try {
    await listen('build-output', (event) => {
      const payload = event.payload;
      if (payload && typeof payload === 'object' && 'line' in payload) {
        appendBuildLogForTab(payload.tab_id, payload.line);
      } else {
        appendBuildLogForTab(activeTabId, String(payload));
      }
    });
  } catch (err) {
    console.error('Failed to attach build-output listener:', err);
  }

  try {
    await listen('cmd-size-picker-done', async (event) => {
      const payload = event.payload;
      if (!payload || typeof payload !== 'object') return;
      const width = parseInt(payload.width, 10);
      const height = parseInt(payload.height, 10);
      if (!Number.isFinite(width) || !Number.isFinite(height)) return;

      const widthEl = document.getElementById('setting-ps-width');
      const heightEl = document.getElementById('setting-ps-height');
      const capEl = document.getElementById('setting-auto-capture');
      const saveImgEl = document.getElementById('setting-auto-save-image');
      const dirEl = document.getElementById('setting-screenshot-dir');
      if (widthEl) widthEl.value = width;
      if (heightEl) heightEl.value = height;

      const autoCapVal = capEl ? !!capEl.checked : !!userSetting.auto_capture_clipboard;
      const autoSaveImgVal = saveImgEl ? !!saveImgEl.checked : !!userSetting.auto_save_image;
      const rawDir = dirEl ? (dirEl.value || '').trim() : userSetting.screenshot_save_dir || '';
      const isDefaultDir =
        defaultScreenshotRootDir &&
        rawDir.replace(/[\\/]+$/, '').toLowerCase() ===
          defaultScreenshotRootDir.replace(/[\\/]+$/, '').toLowerCase();
      const saveDirVal = !rawDir || isDefaultDir ? null : rawDir;

      try {
        const saved = await invoke('save_user_setting', {
          setting: {
            ps_window_width: width,
            ps_window_height: height,
            auto_capture_clipboard: autoCapVal,
            auto_save_image: autoSaveImgVal,
            screenshot_save_dir: saveDirVal,
          },
        });
        userSetting = saved;
        if (widthEl) widthEl.value = saved.ps_window_width;
        if (heightEl) heightEl.value = saved.ps_window_height;
        updateFooterConfigVal();
      } catch (err) {
        console.error('Failed to persist picked CMD size:', err);
      }
    });
  } catch (err) {
    console.error('Failed to attach cmd-size-picker-done listener:', err);
  }

  try {
    await listen('bat-execution-finished', (event) => {
      const payload = event.payload;
      if (!payload) return;

      const targetTab = (payload.tab_id && getTabById(payload.tab_id)) || getActiveTab();
      if (targetTab && payload.path) {
        setBatExecStatus(targetTab, payload.path, payload.success ? 'success' : 'failed', payload);
        if (targetTab.id === activeTabId) {
          applyBatFilterAndRender(targetTab);
        }
        if (payload.screenshot_saved && payload.screenshot_path) {
          setTabFooterStatus(
            targetTab,
            `Screenshot saved: ${payload.bat_name || 'batch'}`,
            formatCurrentDateTime(),
            'ok'
          );
        } else if (!payload.success) {
          setTabFooterStatus(
            targetTab,
            `BAT Failed: ${payload.bat_name || 'batch'}`,
            formatCurrentDateTime(),
            'err'
          );
        }
      }

      if (payload.screenshot_error) {
        showAlert(
          'Auto Save Image Failed',
          `<b>File:</b> ${escapeHtml(payload.bat_name || '')}<br><b>Destination:</b> ${escapeHtml(payload.screenshot_path || '')}<br><b>Error:</b> ${escapeHtml(payload.screenshot_error || 'Unknown error')}`,
          '[!]'
        );
      }
    });
  } catch (err) {
    console.error('Failed to attach bat-execution-finished listener:', err);
  }

  await Promise.allSettled([
    loadSettings(),
    loadUserSetting(),
    loadEnvironment(),
    loadAntOptions(),
    loadJdkOptions(),
  ]);
}

// Simulated CMD Window Size Calibration Mode (runs inside cmd-size-picker window)
async function initCmdSizePickerMode() {
  document.title = 'Command Prompt - Size Preview';
  document.body.className = 'cmd-picker-body';
  document.body.innerHTML = `
    <div class="cmd-picker-shell">
      <div class="cmd-picker-terminal" title="Drag background to move window; drag window edges/corners to resize">
<span class="cmd-picker-prompt">PS C:\\...\\target\\batch\\batfiles&gt;</span> .\\SampleBatchJob.bat
Microsoft Windows [Version 10.0]
Batch Helper - Simulated Command Prompt Window Size Preview

[INFO] 2026-11-19 10:00:00  Initializing BatchController...
[INFO] 2026-11-19 10:00:01  Drag any edge or corner of this window to fit your target log output.
[INFO] 2026-11-19 10:00:02  Click [DONE] below to apply this exact width &amp; height (px) to BAT Run Settings.

<span class="cmd-picker-ruler">----+----1----+----2----+----3----+----4----+----5----+----6----+----7----+----8----+----9----+----0</span>
1234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234567890
      </div>
      <div class="cmd-picker-footer">
        <span class="cmd-picker-footer-info" data-role="dim-info">Resize window edges/corners, then click DONE.</span>
        <div class="cmd-picker-footer-actions">
          <span class="cmd-picker-dim-badge" data-role="dim-badge">-- x -- px</span>
          <button type="button" class="win95-btn" data-role="btn-cancel">Cancel</button>
          <button type="button" class="win95-btn cmd-picker-done-btn" data-role="btn-done">DONE</button>
        </div>
      </div>
    </div>
  `;

  const dimBadge = document.body.querySelector('[data-role="dim-badge"]');
  const dimInfo = document.body.querySelector('[data-role="dim-info"]');
  const btnCancel = document.body.querySelector('[data-role="btn-cancel"]');
  const btnDone = document.body.querySelector('[data-role="btn-done"]');
  const terminalEl = document.body.querySelector('.cmd-picker-terminal');

  const updateReadout = (dims) => {
    if (!dims) return;
    const text = `${dims.width} x ${dims.height} px`;
    if (dimBadge) dimBadge.textContent = text;
    if (dimInfo) {
      dimInfo.textContent = `Drag edges/corners to resize. Current window size: ${text}`;
    }
  };

  const refreshDimensions = async () => {
    try {
      const dims = await invoke('get_cmd_size_picker_dimensions');
      updateReadout(dims);
    } catch (_) {}
  };

  try {
    await listen('cmd-size-picker-resized', (event) => {
      updateReadout(event.payload);
    });
  } catch (_) {}

  window.addEventListener('resize', () => {
    refreshDimensions();
  });

  const handleDragMouseDown = (e) => {
    if (e.button !== 0) return;
    if (e.target && e.target.closest('button')) return;
    invoke('start_drag_cmd_size_picker').catch(() => {});
  };
  if (terminalEl) terminalEl.addEventListener('mousedown', handleDragMouseDown);

  const confirmAndClose = async () => {
    try {
      await invoke('confirm_cmd_size_picker');
    } catch (err) {
      console.error('Failed to confirm CMD size:', err);
    }
  };
  const cancelAndClose = async () => {
    try {
      await invoke('cancel_cmd_size_picker');
    } catch (err) {
      console.error('Failed to cancel CMD size picker:', err);
    }
  };

  if (btnDone) btnDone.addEventListener('click', confirmAndClose);
  if (btnCancel) btnCancel.addEventListener('click', cancelAndClose);

  document.addEventListener('keydown', (e) => {
    if (e.key === 'Escape') {
      e.preventDefault();
      cancelAndClose();
    } else if (e.key === 'Enter') {
      e.preventDefault();
      confirmAndClose();
    }
  });

  try {
    const initialDims = await invoke('ready_cmd_size_picker');
    updateReadout(initialDims);
  } catch (err) {
    console.error('Failed to show simulated CMD window:', err);
  }
}

window.addEventListener('DOMContentLoaded', () => {
  let winLabel = 'main';
  try {
    winLabel = getCurrentWebviewWindow().label;
  } catch (_) {
    winLabel = 'main';
  }

  if (winLabel === 'cmd-size-picker') {
    initCmdSizePickerMode();
  } else {
    init();
  }
});
