// ==========================================================================
// Batch Helper - Main Application Logic (Win95 Multi-Tab Edition)
// ==========================================================================

import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

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

function createTabState(projectPath = '') {
  tabCounter += 1;
  return {
    id: `tab_${Date.now()}_${tabCounter}`,
    projectPath: projectPath || '',
    buildDir: '',
    targets: [],
    defaultTarget: '',
    selectedTarget: '',
    buildStatus: 'READY',
    buildStatusClass: 'badge-gray',
    isBuilding: false,
    consoleLogs: '',
    batFolder: '',
    batFilter: '',
    batFiles: [],
    logFile: null,
    logFolder: null,
  };
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
  }
  updateHistoryDropdown(historyListCache, tab.projectPath);

  // 2. Target Dropdown & Build Button
  const targetSelect = document.getElementById('select-ant-target');
  const buildBtn = document.getElementById('btn-run-build');
  if (targetSelect) {
    targetSelect.innerHTML = '';
    if (!tab.projectPath) {
      const opt = document.createElement('option');
      opt.value = '';
      opt.textContent = '-- Select a project to load targets --';
      targetSelect.appendChild(opt);
      if (buildBtn) buildBtn.disabled = true;
    } else if (!tab.targets || tab.targets.length === 0) {
      const opt = document.createElement('option');
      opt.value = '';
      opt.textContent = '(No targets found)';
      targetSelect.appendChild(opt);
      if (buildBtn) buildBtn.disabled = true;
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
      if (buildBtn) buildBtn.disabled = tab.isBuilding;
    }
  }

  // 3. Build Status Badge
  const badgeStatus = document.getElementById('badge-build-status');
  if (badgeStatus) {
    badgeStatus.textContent = tab.buildStatus || 'READY';
    badgeStatus.className = `badge ${tab.buildStatusClass || 'badge-gray'}`;
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
  }
  if (btnOpenBat) {
    btnOpenBat.disabled = !tab.batFolder;
  }
  if (inputBatFilter) {
    inputBatFilter.value = tab.batFilter || '';
  }

  applyBatFilterAndRender(tab);
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

    // Scan all restored tabs
    for (const tab of projectTabs) {
      if (tab.projectPath && tab.projectPath.trim() !== '') {
        await scanProjectForTab(tab, tab.projectPath);
      }
    }
  } catch (err) {
    console.error('Error loading settings:', err);
  }
}

function updateHistoryDropdown(historyList, selectedProject) {
  const select = document.getElementById('select-project-history');
  if (!select) return;

  select.innerHTML = '';
  if (!historyList || historyList.length === 0) {
    const opt = document.createElement('option');
    opt.value = '';
    opt.textContent = '(No history)';
    select.appendChild(opt);
    return;
  }

  const defaultOpt = document.createElement('option');
  defaultOpt.value = '';
  defaultOpt.textContent = '-- Select recent project --';
  select.appendChild(defaultOpt);

  historyList.forEach((item) => {
    const opt = document.createElement('option');
    opt.value = item;
    const parts = item.split(/[\\/]/).filter(Boolean);
    const shortName = parts.length > 0 ? parts[parts.length - 1] : item;
    opt.textContent = `${shortName} (${item})`;
    opt.title = item;
    if (selectedProject && item.toLowerCase() === selectedProject.toLowerCase()) {
      opt.selected = true;
    }
    select.appendChild(opt);
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
  tab.projectPath = cleanPath;
  renderTabsBar();

  const footerStatus = document.getElementById('footer-status-text');
  if (tab.id === activeTabId) {
    footerStatus.textContent = 'Scanning...';
  }

  try {
    const res = await invoke('scan_project_dir', { path: cleanPath });

    if (!res.is_valid_directory) {
      if (tab.id === activeTabId) {
        showAlert('Invalid Directory', res.error_message || 'Directory is not valid.', '[X]');
        footerStatus.textContent = 'Error';
      }
      return;
    }

    if (res.build_folder_found && res.build_folder_path) {
      tab.buildDir = res.build_folder_path;
    } else {
      tab.buildDir = cleanPath;
    }

    if (!res.build_xml_found || res.ant_targets.length === 0) {
      tab.targets = [];
      tab.selectedTarget = '';
      if (tab.id === activeTabId) {
        syncUIToActiveTab();
        footerStatus.textContent = !res.build_xml_found ? 'No build.xml' : 'No targets';
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

    await inspectTargetBatFilesForTab(tab, cleanPath, tab.selectedTarget);

    if (tab.id === activeTabId) {
      syncUIToActiveTab();
      footerStatus.textContent = 'Ready';
    }
  } catch (err) {
    console.error('Scan project failed:', err);
    if (tab.id === activeTabId) {
      showAlert('Error', `Failed to scan project: ${err}`, '[X]');
      footerStatus.textContent = 'Error';
    }
  }
}

// ==========================================================================
// 3. INSPECT TARGET BAT FILES & CUSTOM FOLDER / FILTER (PER TAB)
// ==========================================================================
async function inspectTargetBatFilesForTab(tab, projectPath, target) {
  if (!tab || !projectPath || !target) return;

  try {
    const inspection = await invoke('inspect_build_target', { projectPath, target });

    tab.batFolder = inspection.bat_folder || '';
    tab.logFile = inspection.log_file_path || null;
    tab.logFolder = inspection.log_folder_path || null;
    tab.batFiles = inspection.bat_files || [];

    if (tab.id === activeTabId) {
      const inputBatFolder = document.getElementById('input-bat-folder');
      const btnOpenBat = document.getElementById('btn-open-bat-dir');
      if (inputBatFolder) {
        inputBatFolder.value = tab.batFolder;
        inputBatFolder.title = tab.batFolder
          ? `Base Dir: ${inspection.base_directory || 'N/A'}\nConfig: ${inspection.properties_file || 'N/A'}`
          : '';
      }
      if (btnOpenBat) {
        btnOpenBat.disabled = !tab.batFolder;
      }
      applyBatFilterAndRender(tab);
    }
  } catch (err) {
    console.error('Inspect bat files failed:', err);
  }
}

async function scanCustomBatFolderForActiveTab(folderPath) {
  const tab = getActiveTab();
  if (!tab || !folderPath || folderPath.trim() === '') return;

  try {
    const [resolvedDir, files] = await invoke('scan_custom_bat_dir', {
      folderPath: folderPath.trim(),
    });
    tab.batFolder = resolvedDir || folderPath.trim();
    tab.batFiles = files || [];

    const inputBatFolder = document.getElementById('input-bat-folder');
    const btnOpenBat = document.getElementById('btn-open-bat-dir');
    if (inputBatFolder && resolvedDir) {
      inputBatFolder.value = resolvedDir;
    }
    if (btnOpenBat) {
      btnOpenBat.disabled = !tab.batFolder;
    }
    applyBatFilterAndRender(tab);
  } catch (err) {
    console.error('Custom bat folder scan failed:', err);
    showAlert('Error', `Failed to scan folder: ${err}`, '[X]');
  }
}

function applyBatFilterAndRender(tab = getActiveTab()) {
  if (!tab || tab.id !== activeTabId) return;

  const filterInput = document.getElementById('input-bat-filter');
  const query = filterInput ? filterInput.value.trim().toLowerCase() : '';
  tab.batFilter = filterInput ? filterInput.value : '';

  const allFiles = tab.batFiles || [];
  const filtered = query
    ? allFiles.filter((f) => f.name.toLowerCase().includes(query))
    : allFiles;

  renderBatFilesTable(filtered, allFiles.length);
}

function renderBatFilesTable(files, totalCount) {
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
  files.forEach((file, idx) => {
    const tr = document.createElement('tr');
    tr.title = `Double-click or click [Run] to execute in PowerShell: ${file.path}`;

    tr.innerHTML = `
      <td style="text-align: center; color: #666;">${idx + 1}</td>
      <td>
        <div class="file-name-cell">
          <b>${file.name}</b>
        </div>
      </td>
      <td style="text-align: center; font-size: 10px; color: #444;">${file.modified_time}</td>
      <td style="text-align: center;">
        <button class="win95-btn btn-bat-run" data-path="${file.path}">
          Run
        </button>
      </td>
    `;

    tr.addEventListener('dblclick', () => {
      runBatScript(file.path);
    });

    tbody.appendChild(tr);
  });

  tbody.querySelectorAll('.btn-bat-run').forEach((btn) => {
    btn.addEventListener('click', (e) => {
      e.stopPropagation();
      const p = btn.getAttribute('data-path');
      runBatScript(p);
    });
  });
}

async function runBatScript(path) {
  try {
    await invoke('execute_bat_file', { path });
  } catch (err) {
    showAlert('Execution Error', `Failed to run script: ${err}`, '[X]');
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
  tab.buildStatus = 'BUILDING...';
  tab.buildStatusClass = 'badge-yellow';
  tab.consoleLogs = '';

  renderTabsBar();
  syncUIToActiveTab();

  const footerStatus = document.getElementById('footer-status-text');
  footerStatus.textContent = 'Building...';

  try {
    const res = await invoke('start_ant_build', {
      tabId: tab.id,
      projectPath: tab.projectPath,
      target: target,
      antExecutable: selectedAntPath,
      javaHome: selectedJavaHome,
    });

    tab.batFolder = res.bat_folder || '';
    tab.logFile = res.log_file_path || null;
    tab.logFolder = res.log_folder_path || null;
    tab.batFiles = res.bat_files || [];

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

    const updateJdkSelection = () => {
      const chosen = res.jdks.find((j) => j.id === select.value);
      if (chosen && chosen.root_path) {
        selectedJavaHome = chosen.root_path;
        if (descEl) {
          descEl.textContent = `Path: ${chosen.root_path}`;
          descEl.title = chosen.root_path;
        }
      }
    };

    select.onchange = updateJdkSelection;
    updateJdkSelection();
  } catch (err) {
    console.error('Scan JDKs failed:', err);
  }
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
        document.getElementById('project-path-input').value = folder;
        const newSettings = await invoke('add_project_history', { path: folder });
        historyListCache = newSettings.history || [];
        updateHistoryDropdown(historyListCache, folder);
        await scanProjectForTab(tab, folder);
        await persistTabsToHistory();
      }
    } catch (err) {
      console.error('Browse folder error:', err);
    }
  });

  // 2. Select from History
  document.getElementById('select-project-history').addEventListener('change', async (e) => {
    const val = e.target.value;
    if (val && val.trim() !== '') {
      const tab = getActiveTab() || addNewProjectTab(val);
      document.getElementById('project-path-input').value = val;
      const newSettings = await invoke('add_project_history', { path: val });
      historyListCache = newSettings.history || [];
      updateHistoryDropdown(historyListCache, val);
      await scanProjectForTab(tab, val);
      await persistTabsToHistory();
    }
  });

  // 3. Enter key or change in project path input
  const inputPathEl = document.getElementById('project-path-input');
  const triggerPathScan = async () => {
    const path = inputPathEl.value;
    const tab = getActiveTab();
    if (tab && path && path.trim() !== '' && path.trim() !== tab.projectPath) {
      const newSettings = await invoke('add_project_history', { path: path.trim() });
      historyListCache = newSettings.history || [];
      updateHistoryDropdown(historyListCache, path.trim());
      await scanProjectForTab(tab, path.trim());
      await persistTabsToHistory();
    }
  };

  inputPathEl.addEventListener('keydown', async (e) => {
    if (e.key === 'Enter') {
      await triggerPathScan();
    }
  });
  inputPathEl.addEventListener('change', async () => {
    await triggerPathScan();
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

  // 5. Run Build Button
  document.getElementById('btn-run-build').addEventListener('click', async () => {
    await executeBuildForActiveTab();
  });

  // 6. Open Build Directory
  document.getElementById('btn-open-build-dir').addEventListener('click', async () => {
    const tab = getActiveTab();
    if (tab && tab.buildDir) {
      await invoke('open_path_in_explorer', { path: tab.buildDir });
    }
  });

  // 7. Open File Log (batchApp.log)
  document.getElementById('btn-open-log').addEventListener('click', async () => {
    const tab = getActiveTab();
    try {
      await invoke('open_file_log', {
        logFilePath: tab ? tab.logFile : null,
        logFolderPath: tab ? tab.logFolder : null,
        projectPath: tab && tab.projectPath ? tab.projectPath : null,
      });
    } catch (err) {
      showAlert('Log File Not Found', `${err}`, '[!]');
    }
  });

  // 8. Open Batch Directory in Explorer
  document.getElementById('btn-open-bat-dir').addEventListener('click', async () => {
    const tab = getActiveTab();
    if (tab && tab.batFolder) {
      await invoke('open_path_in_explorer', { path: tab.batFolder });
    }
  });

  // 9. Custom Batch Folder Input & Browse
  const inputBatFolder = document.getElementById('input-bat-folder');
  inputBatFolder.addEventListener('keydown', async (e) => {
    if (e.key === 'Enter') {
      await scanCustomBatFolderForActiveTab(inputBatFolder.value);
    }
  });
  inputBatFolder.addEventListener('change', async () => {
    await scanCustomBatFolderForActiveTab(inputBatFolder.value);
  });

  document.getElementById('btn-browse-bat-folder').addEventListener('click', async () => {
    try {
      const folder = await invoke('pick_folder');
      if (folder) {
        inputBatFolder.value = folder;
        await scanCustomBatFolderForActiveTab(folder);
      }
    } catch (err) {
      console.error('Browse bat folder error:', err);
    }
  });

  // 10. Search / Filter Batch Files
  document.getElementById('input-bat-filter').addEventListener('input', () => {
    applyBatFilterAndRender();
  });

  // 11. Footer Click -> Open Modals
  document.getElementById('footer-ant-btn').addEventListener('click', () => {
    document.getElementById('modal-ant').style.display = 'flex';
  });

  document.getElementById('footer-java-btn').addEventListener('click', () => {
    document.getElementById('modal-java').style.display = 'flex';
  });

  // Modal Close buttons
  document.getElementById('btn-close-modal-ant').addEventListener('click', () => {
    document.getElementById('modal-ant').style.display = 'none';
  });
  document.getElementById('modal-btn-close-ant').addEventListener('click', () => {
    document.getElementById('modal-ant').style.display = 'none';
  });

  document.getElementById('btn-close-modal-java').addEventListener('click', () => {
    document.getElementById('modal-java').style.display = 'none';
  });
  document.getElementById('modal-btn-close-java').addEventListener('click', () => {
    document.getElementById('modal-java').style.display = 'none';
  });

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
        selectedJavaHome = folder;
        const descEl = document.getElementById('modal-jdk-choice-desc');
        if (descEl) descEl.textContent = `Custom Path: ${folder}`;
      }
    } catch (err) {
      console.error(err);
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

  await Promise.allSettled([
    loadSettings(),
    loadEnvironment(),
    loadAntOptions(),
    loadJdkOptions(),
  ]);
}

window.addEventListener('DOMContentLoaded', () => {
  init();
});
