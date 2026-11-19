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
};

let batSplitLeftPct = 40;
let batViewerDarkTheme = false;
let groupRunSteps = [];
let groupRunSelectedIdx = 0;
let activeGroupRunLocked = false;

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
  const modalIds = ['modal-ant', 'modal-java', 'modal-settings', 'modal-auto-config'];
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

function createTabState(projectPath = '') {
  tabCounter += 1;
  return {
    id: `tab_${Date.now()}_${tabCounter}`,
    requestSeq: 0,
    projectPath: projectPath || '',
    buildDir: '',
    targets: [],
    defaultTarget: '',
    selectedTarget: '',
    propertiesFile: null,
    buildStatus: 'READY',
    buildStatusClass: 'badge-gray',
    isBuilding: false,
    isGroupRunning: false,
    cmdPanelOpen: false,
    cmdPanelExpanded: false,
    consoleLogs: '',
    batFolder: '',
    batFilter: '',
    batFiles: [],
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
  tab.targets = [];
  tab.defaultTarget = '';
  tab.selectedTarget = '';
  tab.propertiesFile = null;
  tab.batFolder = '';
  tab.batFiles = [];
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

  updateLogButtonsUI(tab);
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

    // Restore saved tabs or fallback to last_project
    const savedTabs = (settings.open_tabs || []).filter((p) => p && p.trim() !== '');
    if (savedTabs.length > 0) {
      savedTabs.forEach((p) => {
        projectTabs.push(createTabState(p));
      });
      const activeIdx = Math.min(
        settings.active_tab_index || 0,
        projectTabs.length - 1
      );
      activeTabId = projectTabs[activeIdx].id;
    } else if (settings.last_project && settings.last_project.trim() !== '') {
      const tab = createTabState(settings.last_project.trim());
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
  const labelEl = document.getElementById('history-dropdown-label');
  const menuEl = document.getElementById('history-dropdown-menu');
  if (!labelEl || !menuEl) return;

  const list = historyList || [];
  if (selectedProject && selectedProject.trim() !== '') {
    const parts = selectedProject.trim().split(/[\\/]/).filter(Boolean);
    const shortName = parts.length > 0 ? parts[parts.length - 1] : selectedProject;
    labelEl.textContent = `${shortName} (${selectedProject})`;
    labelEl.title = selectedProject;
  } else if (list.length === 0) {
    labelEl.textContent = '(No history)';
    labelEl.title = '';
  } else {
    labelEl.textContent = '-- Select recent project --';
    labelEl.title = '';
  }

  menuEl.innerHTML = '';
  if (list.length === 0) {
    const emptyDiv = document.createElement('div');
    emptyDiv.className = 'history-empty-row';
    emptyDiv.textContent = '(No history)';
    menuEl.appendChild(emptyDiv);
    return;
  }

  list.forEach((item) => {
    const row = document.createElement('div');
    const isActive =
      selectedProject && item.toLowerCase() === selectedProject.trim().toLowerCase();
    row.className = `history-item-row${isActive ? ' active' : ''}`;
    row.title = item;

    const textSpan = document.createElement('span');
    textSpan.className = 'history-item-text';
    const parts = item.split(/[\\/]/).filter(Boolean);
    const shortName = parts.length > 0 ? parts[parts.length - 1] : item;
    textSpan.textContent = `${shortName} (${item})`;

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
      const tab = getActiveTab() || addNewProjectTab(item);
      if (tab.id === activeTabId) {
        const inputEl = document.getElementById('project-path-input');
        if (inputEl) {
          inputEl.value = item;
          inputEl.dataset.tabId = tab.id;
        }
      }
      const newSettings = await invoke('add_project_history', { path: item });
      historyListCache = newSettings.history || [];
      if (tab.id === activeTabId) {
        updateHistoryDropdown(historyListCache, item);
      }
      await scanProjectForTab(tab, item);
      await persistTabsToHistory();
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

  const isSwitchingProject = cleanPath.toLowerCase() !== (tab.projectPath || '').trim().toLowerCase();
  if (isSwitchingProject) {
    resetTabProjectState(tab);
  }
  tab.projectPath = cleanPath;
  renderTabsBar();
  if (isSwitchingProject && tab.id === activeTabId) {
    syncUIToActiveTab();
  }

  const footerStatus = document.getElementById('footer-status-text');
  if (tab.id === activeTabId && footerStatus) {
    footerStatus.textContent = 'Scanning...';
  }

  try {
    const res = await invoke('scan_project_dir', { path: cleanPath });
    if (tab.requestSeq !== reqSeq) return;

    if (!res.is_valid_directory) {
      resetTabProjectState(tab);
      if (tab.id === activeTabId) {
        syncUIToActiveTab();
        showAlert('Invalid Directory', res.error_message || 'Directory is not valid.', '[X]');
        if (footerStatus) footerStatus.textContent = 'Error';
      }
      return;
    }

    if (res.build_folder_found && res.build_folder_path) {
      tab.buildDir = res.build_folder_path;
    } else {
      tab.buildDir = cleanPath;
    }

    if (!res.build_xml_found || res.ant_targets.length === 0) {
      resetTabProjectState(tab);
      tab.buildDir = res.build_folder_found && res.build_folder_path ? res.build_folder_path : cleanPath;
      if (tab.id === activeTabId) {
        syncUIToActiveTab();
        if (footerStatus) {
          footerStatus.textContent = !res.build_xml_found ? 'No build.xml' : 'No targets';
        }
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

    await inspectTargetBatFilesForTab(tab, cleanPath, tab.selectedTarget, reqSeq);
    if (tab.requestSeq !== reqSeq) return;

    if (tab.id === activeTabId) {
      syncUIToActiveTab();
      if (footerStatus) footerStatus.textContent = 'Ready';
    }
  } catch (err) {
    console.error('Scan project failed:', err);
    if (tab.requestSeq === reqSeq && tab.id === activeTabId) {
      showAlert('Error', `Failed to scan project: ${err}`, '[X]');
      if (footerStatus) footerStatus.textContent = 'Error';
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

async function inspectTargetBatFilesForTab(tab, projectPath, target, expectedSeq = null) {
  if (!tab || !projectPath || !target) return;

  const reqSeq = expectedSeq !== null ? expectedSeq : (tab.requestSeq || 0) + 1;
  if (expectedSeq === null) {
    tab.requestSeq = reqSeq;
  }

  try {
    const inspection = await invoke('inspect_build_target', { projectPath, target });
    if (
      tab.requestSeq !== reqSeq ||
      (tab.projectPath || '').trim().toLowerCase() !== projectPath.trim().toLowerCase() ||
      tab.selectedTarget !== target
    ) {
      return;
    }

    tab.propertiesFile = inspection.properties_file || null;
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
      updateLogButtonsUI(tab);
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
  const viewer = document.getElementById('bat-preview-content');
  const toggleBtn = document.getElementById('btn-toggle-bat-theme');
  if (viewer) {
    viewer.classList.toggle('theme-dark', !!batViewerDarkTheme);
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

function getFilteredBatFiles(tab = getActiveTab()) {
  if (!tab) return [];
  const filterInput = document.getElementById('input-bat-filter');
  const query =
    tab.id === activeTabId && filterInput
      ? filterInput.value.trim().toLowerCase()
      : (tab.batFilter || '').trim().toLowerCase();
  const allFiles = tab.batFiles || [];
  return query ? allFiles.filter((f) => f.name.toLowerCase().includes(query)) : allFiles;
}

function getSelectedFilesInDisplayOrder(tab = getActiveTab()) {
  if (!tab) return [];
  const selectedSet = new Set((tab.selectedBatPaths || []).map((p) => p.toLowerCase()));
  if (selectedSet.size === 0 && tab.selectedBatPath) {
    selectedSet.add(tab.selectedBatPath.toLowerCase());
  }
  const filtered = getFilteredBatFiles(tab);
  const inFiltered = filtered.filter((f) => selectedSet.has(f.path.toLowerCase()));
  if (inFiltered.length > 0) return inFiltered;
  return (tab.batFiles || []).filter((f) => selectedSet.has(f.path.toLowerCase()));
}

function updateSelectedCountBadge(tab = getActiveTab()) {
  const badge = document.getElementById('meta-bat-selected');
  if (!badge) return;
  const count = tab && tab.selectedBatPaths ? tab.selectedBatPaths.length : 0;
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

function showBatContextMenu(x, y, tab) {
  const menu = document.getElementById('bat-context-menu');
  if (!menu || !tab) return;

  const selectedFiles = getSelectedFilesInDisplayOrder(tab);
  if (selectedFiles.length === 0) return;

  menu.innerHTML = '';

  const addItem = (label, onClick, disabled = false, bold = false) => {
    const div = document.createElement('div');
    div.className = `win95-context-item${disabled ? ' disabled' : ''}`;
    if (bold) div.style.fontWeight = 'bold';
    div.textContent = label;
    if (!disabled) {
      div.addEventListener('click', (e) => {
        e.stopPropagation();
        closeBatContextMenu();
        onClick();
      });
    }
    menu.appendChild(div);
  };

  const addSep = () => {
    const sep = document.createElement('div');
    sep.className = 'win95-context-sep';
    menu.appendChild(sep);
  };

  if (selectedFiles.length > 1) {
    addItem(
      `Run Group (${selectedFiles.length} files)`,
      () => runBatGroupSequential(tab, selectedFiles),
      activeGroupRunLocked || tab.isGroupRunning,
      true
    );
    addSep();
    addItem('Open File (Text Editor)', () => {
      if (tab.selectedBatPath) {
        openBatInTextEditor(tab.selectedBatPath);
      }
    });
    addItem('Clear Multi-Selection', () => {
      tab.selectedBatPaths = tab.selectedBatPath ? [tab.selectedBatPath] : [];
      applyBatFilterAndRender(tab);
    });
  } else {
    const single = selectedFiles[0];
    addItem(`Run (${single.name})`, () => runBatScript(single.path), false, true);
    addItem('Open File (Text Editor)', () => openBatInTextEditor(single.path));
  }

  menu.style.display = 'block';
  const menuW = menu.offsetWidth || 190;
  const menuH = menu.offsetHeight || 90;
  const clampedX = Math.max(4, Math.min(x, window.innerWidth - menuW - 6));
  const clampedY = Math.max(4, Math.min(y, window.innerHeight - menuH - 6));
  menu.style.left = `${clampedX}px`;
  menu.style.top = `${clampedY}px`;
}

function applyBatFilterAndRender(tab = getActiveTab()) {
  if (!tab || tab.id !== activeTabId) return;

  const filterInput = document.getElementById('input-bat-filter');
  if (filterInput) {
    tab.batFilter = filterInput.value;
  }

  const allFiles = tab.batFiles || [];
  const filtered = getFilteredBatFiles(tab);
  updateSelectedCountBadge(tab);
  renderBatFilesTable(filtered, allFiles.length, tab);
}

function handleBatRowSelection(e, tab, file, idx, displayedFiles) {
  if (!tab || !file) return;

  const filePathLower = file.path.toLowerCase();
  const isShift = !!e.shiftKey;
  const isCtrl = !!(e.ctrlKey || e.metaKey);

  if (isShift && tab.lastSelectedBatIndex >= 0 && tab.lastSelectedBatIndex < displayedFiles.length) {
    if (window.getSelection) {
      window.getSelection().removeAllRanges();
    }
    const start = Math.min(tab.lastSelectedBatIndex, idx);
    const end = Math.max(tab.lastSelectedBatIndex, idx);
    const rangePaths = displayedFiles.slice(start, end + 1).map((f) => f.path);

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
    const existing = tab.selectedBatPaths || [];
    const foundIdx = existing.findIndex((p) => p.toLowerCase() === filePathLower);
    if (foundIdx >= 0) {
      tab.selectedBatPaths = existing.filter((_, i) => i !== foundIdx);
    } else {
      tab.selectedBatPaths = [...existing, file.path];
    }
    tab.lastSelectedBatIndex = idx;
  } else {
    tab.selectedBatPaths = [file.path];
    tab.lastSelectedBatIndex = idx;
  }

  updateSelectedCountBadge(tab);
  loadBatFileContentForTab(tab, file);
}

function renderBatFilesTable(files, totalCount, tab) {
  const tbody = document.getElementById('tbody-bat-files');
  const metaCount = document.getElementById('meta-bat-count');

  if (!totalCount || totalCount === 0) {
    tbody.innerHTML = `
      <tr>
        <td colspan="4" class="empty-row">
          No .bat files found in target directory. Run Build or browse a custom folder.
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
      tr.className = 'selected-row';
    }
    tr.title = `Click to preview | Ctrl/Shift+Click multi-select | Right-click menu | Dbl-click Run: ${file.path}`;

    tr.innerHTML = `
      <td style="text-align: center;">${idx + 1}</td>
      <td title="${escapeHtml(file.name)}">
        <div class="file-name-cell">
          <b>${escapeHtml(file.name)}</b>
        </div>
      </td>
      <td style="text-align: center; font-size: 10px;">${escapeHtml(file.modified_time)}</td>
      <td style="text-align: center;">
        <button class="win95-btn btn-bat-run" data-path="${escapeHtml(file.path)}">
          Run
        </button>
      </td>
    `;

    tr.addEventListener('click', (e) => {
      closeBatContextMenu();
      const activeTab = getActiveTab();
      if (activeTab) {
        handleBatRowSelection(e, activeTab, file, idx, files);
      }
    });

    tr.addEventListener('dblclick', (e) => {
      closeBatContextMenu();
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
      const activeTab = getActiveTab();
      if (!activeTab) return;

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
      const p = btn.getAttribute('data-path');
      runBatScript(p);
    });
  });
}

async function runBatScript(path) {
  try {
    await invoke('execute_bat_file', { path });
  } catch (err) {
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
    if (idx === groupRunSelectedIdx) {
      tr.className = 'selected-row';
    }

    let badgeHtml = '<span class="badge badge-gray">PENDING</span>';
    if (step.status === 'RUNNING') {
      badgeHtml = '<span class="badge badge-yellow">RUNNING</span>';
    } else if (step.status === 'OK') {
      badgeHtml = '<span class="badge badge-green">OK</span>';
    } else if (step.status === 'FAILED') {
      badgeHtml = '<span class="badge badge-red">FAILED</span>';
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
    `;

    tr.addEventListener('click', () => {
      groupRunSelectedIdx = idx;
      renderGroupRunModal();
    });

    tbody.appendChild(tr);
  });

  const activeStep = groupRunSteps[groupRunSelectedIdx] || null;
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

async function runBatGroupSequential(tab, filesInOrder) {
  if (!tab || !filesInOrder || filesInOrder.length === 0) return;
  if (activeGroupRunLocked || tab.isGroupRunning) {
    showAlert('Group Run Busy', 'A batch group execution is already in progress.', '[!]');
    return;
  }

  activeGroupRunLocked = true;
  tab.isGroupRunning = true;

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
  }));
  groupRunSelectedIdx = 0;

  openSingleModal('modal-group-run');
  renderGroupRunModal();

  let passedCount = 0;
  let failedCount = 0;
  const total = filesInOrder.length;

  try {
    for (let i = 0; i < total; i++) {
      const file = filesInOrder[i];
      groupRunSteps[i].status = 'RUNNING';
      groupRunSelectedIdx = i;
      if (summaryEl) {
        summaryEl.textContent = `Running step ${i + 1} of ${total}: ${file.name}...`;
      }
      renderGroupRunModal();

      try {
        const stepRes = await invoke('execute_bat_group_step', { path: file.path });
        groupRunSteps[i].duration_ms = stepRes.duration_ms;
        groupRunSteps[i].exit_code = stepRes.exit_code;
        groupRunSteps[i].stdout = stepRes.stdout || '';
        groupRunSteps[i].stderr = stepRes.stderr || '';
        groupRunSteps[i].error_message = stepRes.error_message || null;
        groupRunSteps[i].status = stepRes.success ? 'OK' : 'FAILED';

        if (stepRes.success) {
          passedCount += 1;
          appendBuildLogForTab(
            tab.id,
            `[Group ${i + 1}/${total}] OK: ${file.name} (${stepRes.duration_ms} ms)`
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
        appendBuildLogForTab(
          tab.id,
          `[Group ${i + 1}/${total}] ERROR: ${file.name} - ${err}`
        );
      }

      renderGroupRunModal();
    }
  } finally {
    activeGroupRunLocked = false;
    tab.isGroupRunning = false;
    if (closeBtn) closeBtn.disabled = false;
    if (closeXBtn) closeXBtn.disabled = false;
    if (summaryEl) {
      summaryEl.textContent = `Completed ${total} file(s) sequentially: ${passedCount} succeeded, ${failedCount} failed.`;
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
  tab.isBuilding = true;
  tab.cmdPanelOpen = true;
  tab.buildStatus = 'BUILDING...';
  tab.buildStatusClass = 'badge-yellow';
  tab.consoleLogs = '';

  renderTabsBar();
  syncUIToActiveTab();

  const footerStatus = document.getElementById('footer-status-text');
  footerStatus.textContent = 'Building...';

  const buildProjectPath = tab.projectPath;
  const buildTarget = target;
  const buildSeq = tab.requestSeq || 0;

  try {
    const res = await invoke('start_ant_build', {
      tabId: tab.id,
      projectPath: buildProjectPath,
      target: buildTarget,
      antExecutable: selectedAntPath,
      javaHome: selectedJavaHome,
    });

    if (
      tab.projectPath === buildProjectPath &&
      tab.selectedTarget === buildTarget &&
      (tab.requestSeq || 0) === buildSeq
    ) {
      tab.propertiesFile = res.properties_file || tab.propertiesFile || null;
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
    }

    if (res.success) {
      tab.buildStatus = '\u2714 BUILD SUCCESSFUL';
      tab.buildStatusClass = 'badge-green';
      if (tab.id === activeTabId) footerStatus.textContent = 'Build Successful';
    } else {
      tab.buildStatus = '\u2716 BUILD FAILED';
      tab.buildStatusClass = 'badge-red';
      if (tab.id === activeTabId) footerStatus.textContent = 'Build Failed';
    }
  } catch (err) {
    console.error('Build execution failed:', err);
    tab.buildStatus = '\u2716 BUILD ERROR';
    tab.buildStatusClass = 'badge-red';
    appendBuildLogForTab(tab.id, `[ERROR] ${err}`);
    if (tab.id === activeTabId) {
      footerStatus.textContent = 'Error';
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
    const [setting, settingPath] = await Promise.all([
      invoke('get_user_setting'),
      invoke('get_user_setting_path'),
    ]);
    if (setting) {
      userSetting = setting;
    }
    updateFooterConfigVal();
    const descEl = document.getElementById('modal-setting-file-desc');
    if (descEl && settingPath) {
      descEl.textContent = `File: ${settingPath}`;
      descEl.title = settingPath;
    }
  } catch (err) {
    console.error('Load user setting failed:', err);
  }
}

function updateFooterConfigVal() {
  const valEl = document.getElementById('footer-config-val');
  if (valEl) {
    const capText = userSetting.auto_capture_clipboard ? 'Shot:ON' : 'Shot:OFF';
    valEl.textContent = `${userSetting.ps_window_width}x${userSetting.ps_window_height} (${capText})`;
  }
}

function updateScreenshotToggleBadge() {
  const capEl = document.getElementById('setting-auto-capture');
  const badgeEl = document.getElementById('setting-auto-capture-state');
  if (!capEl || !badgeEl) return;
  const enabled = !!capEl.checked;
  badgeEl.textContent = enabled ? 'ON' : 'OFF';
  badgeEl.className = `badge ${enabled ? 'badge-green' : 'badge-gray'}`;
}

function populateSettingsModal() {
  const widthEl = document.getElementById('setting-ps-width');
  const heightEl = document.getElementById('setting-ps-height');
  const capEl = document.getElementById('setting-auto-capture');
  if (widthEl) widthEl.value = userSetting.ps_window_width;
  if (heightEl) heightEl.value = userSetting.ps_window_height;
  if (capEl) capEl.checked = !!userSetting.auto_capture_clipboard;
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
        summaryEl.textContent =
          'Select a project directory and target package first before applying Auto Config.';
      } else if (antOk && javaOk && winOk) {
        summaryEl.textContent =
          `All 3 checks passed. [Apply Auto Config] will configure build.xml & .properties using Java Home "${effectiveJavaHome}" and encoding-safe short paths.`;
      } else {
        const missing = [];
        if (!antOk) missing.push('Apache Ant');
        if (!javaOk) missing.push('Java (JDK)');
        if (!winOk) missing.push('Window Size');
        summaryEl.textContent =
          `Warning: [${missing.join(', ')}] not ready. Use Config / Setting above to resolve, or apply path updates now.`;
      }
    }
  } catch (err) {
    console.error('Auto config pre-check failed:', err);
    if (summaryEl) {
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
        const tab = getActiveTab() || addNewProjectTab(folder);
        if (tab.id === activeTabId) {
          const inputEl = document.getElementById('project-path-input');
          if (inputEl) {
            inputEl.value = folder;
            inputEl.dataset.tabId = tab.id;
          }
        }
        const newSettings = await invoke('add_project_history', { path: folder });
        historyListCache = newSettings.history || [];
        if (tab.id === activeTabId) {
          updateHistoryDropdown(historyListCache, folder);
        }
        await scanProjectForTab(tab, folder);
        await persistTabsToHistory();
      }
    } catch (err) {
      console.error('Browse folder error:', err);
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

  // 3. Enter key or change in project path input (bound safely to focused tabId)
  const inputPathEl = document.getElementById('project-path-input');
  const triggerPathScan = async (explicitTabId = null) => {
    const cleanPath = (inputPathEl.value || '').trim();
    const tab = (explicitTabId && getTabById(explicitTabId)) || getActiveTab();
    if (tab && cleanPath !== '' && cleanPath !== (tab.projectPath || '').trim()) {
      const newSettings = await invoke('add_project_history', { path: cleanPath });
      historyListCache = newSettings.history || [];
      if (tab.id === activeTabId) {
        updateHistoryDropdown(historyListCache, cleanPath);
      }
      await scanProjectForTab(tab, cleanPath);
      await persistTabsToHistory();
    }
  };

  inputPathEl.addEventListener('focus', () => {
    inputPathEl.dataset.tabId = activeTabId;
  });
  inputPathEl.addEventListener('keydown', async (e) => {
    if (e.key === 'Enter') {
      await triggerPathScan(activeTabId);
    }
  });
  inputPathEl.addEventListener('change', async () => {
    await triggerPathScan(inputPathEl.dataset.tabId || activeTabId);
  });

  // 4. Target selection change
  document.getElementById('select-ant-target').addEventListener('change', async (e) => {
    const tab = getActiveTab();
    if (!tab) return;
    tab.selectedTarget = e.target.value;
    if (tab.projectPath && tab.selectedTarget) {
      await inspectTargetBatFilesForTab(tab, tab.projectPath, tab.selectedTarget);
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

  // 11. Footer Click -> Open Single Non-Overlapping Modal
  document.getElementById('footer-ant-btn').addEventListener('click', () => {
    openSingleModal('modal-ant');
  });

  document.getElementById('footer-java-btn').addEventListener('click', () => {
    openSingleModal('modal-java');
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

  document.getElementById('modal-btn-reset-settings').addEventListener('click', () => {
    const widthEl = document.getElementById('setting-ps-width');
    const heightEl = document.getElementById('setting-ps-height');
    const capEl = document.getElementById('setting-auto-capture');
    if (widthEl) widthEl.value = 1000;
    if (heightEl) heightEl.value = 650;
    if (capEl) capEl.checked = true;
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

    try {
      const saved = await invoke('save_user_setting', {
        setting: {
          ps_window_width: widthVal,
          ps_window_height: heightVal,
          auto_capture_clipboard: autoCapVal,
        },
      });
      userSetting = saved;
      updateFooterConfigVal();
      closeSettingsModal();
      showAlert(
        'Settings Saved',
        `<b>Window Size:</b> ${saved.ps_window_width} x ${saved.ps_window_height} px<br><b>Auto Capture:</b> ${saved.auto_capture_clipboard ? 'ON' : 'OFF'}`,
        '[OK]'
      );
    } catch (err) {
      showAlert('Save Error', `Failed to save setting.json: ${err}`, '[X]');
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
      });

      await inspectTargetBatFilesForTab(tab, tab.projectPath, tab.selectedTarget);
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
      if (widthEl) widthEl.value = width;
      if (heightEl) heightEl.value = height;

      const autoCapVal = capEl ? !!capEl.checked : !!userSetting.auto_capture_clipboard;
      try {
        const saved = await invoke('save_user_setting', {
          setting: {
            ps_window_width: width,
            ps_window_height: height,
            auto_capture_clipboard: autoCapVal,
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
      <div class="cmd-picker-header" title="Drag here or resize window borders to calibrate CMD size">
        <span class="cmd-picker-header-title">C:\\Windows\\System32\\cmd.exe - Batch Run Size Preview</span>
        <span class="cmd-picker-dim-badge" data-role="dim-badge">-- x -- px</span>
      </div>
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
  const headerEl = document.body.querySelector('.cmd-picker-header');
  const terminalEl = document.body.querySelector('.cmd-picker-terminal');

  const updateReadout = (dims) => {
    if (!dims) return;
    const text = `${dims.width} x ${dims.height} px`;
    if (dimBadge) dimBadge.textContent = text;
    if (dimInfo) {
      dimInfo.textContent = `Drag edges/corners to resize. Current outer size: ${text}`;
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
  if (headerEl) headerEl.addEventListener('mousedown', handleDragMouseDown);
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
