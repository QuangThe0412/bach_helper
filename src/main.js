// Batch Helper - Main Application Logic (Windows 95 Edition)

let currentEnvData = null;
let selectedAnt = null;
let selectedJdk = null;
let allAntOptions = [];
let allJdkOptions = [];

// Safe Tauri Invoke Wrapper
async function callRust(cmd, args = {}) {
  try {
    if (window.__TAURI__?.core?.invoke) {
      return await window.__TAURI__.core.invoke(cmd, args);
    }
    const api = await import('@tauri-apps/api/core');
    return await api.invoke(cmd, args);
  } catch (err) {
    console.error(`Command ${cmd} error:`, err);
    throw err;
  }
}

// Log to app terminal
function logActivity(message) {
  const terminal = document.getElementById('terminal-app-log');
  if (!terminal) return;
  const now = new Date();
  const timeStr = now.toTimeString().split(' ')[0];
  const newText = `[${timeStr}] ${message}\n`;
  terminal.textContent += newText;
  terminal.scrollTop = terminal.scrollHeight;
}

// Show Custom Win95 Alert Box
function showWin95Alert(title, message, icon = 'ℹ️') {
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

function closeWin95Alert() {
  const modal = document.getElementById('alert-modal');
  if (modal) modal.style.display = 'none';
}

// Update clock
function updateClock() {
  const now = new Date();
  const timeStr = now.toLocaleTimeString();
  const trayClock = document.getElementById('tray-clock');
  const sbTime = document.getElementById('sb-time');
  if (trayClock) trayClock.textContent = timeStr;
  if (sbTime) sbTime.textContent = timeStr;
}
setInterval(updateClock, 1000);
updateClock();

// Tab navigation
function setupTabs() {
  const tabs = document.querySelectorAll('.win95-tabs li[role="tab"]');
  const panels = document.querySelectorAll('article[role="tabpanel"]');

  tabs.forEach((tab) => {
    tab.addEventListener('click', (e) => {
      e.preventDefault();
      tabs.forEach((t) => t.setAttribute('aria-selected', 'false'));
      tab.setAttribute('aria-selected', 'true');

      const targetId = tab.getAttribute('data-tab');
      panels.forEach((panel) => {
        if (panel.id === targetId) {
          panel.classList.add('active');
        } else {
          panel.classList.remove('active');
        }
      });
    });
  });
}

// TAB 1: PROJECT SCANNING
function setupProjectTab() {
  const inputPath = document.getElementById('project-path-input');
  const btnBrowse = document.getElementById('btn-browse-folder');
  const btnScan = document.getElementById('btn-scan-project');
  const btnSample = document.getElementById('btn-load-sample');

  btnBrowse.addEventListener('click', async () => {
    try {
      logActivity('Opening Windows native folder dialog...');
      const selected = await callRust('pick_folder');
      if (selected) {
        inputPath.value = selected;
        logActivity(`Selected folder: ${selected}`);
        await performProjectScan(selected);
      }
    } catch (err) {
      logActivity(`Browse folder error: ${err}`);
      showWin95Alert('Browse Error', `Failed to open folder picker: ${err}`, '❌');
    }
  });

  btnSample.addEventListener('click', async () => {
    inputPath.value = 'D:\\vinx_tools\\bach_helper\\docs';
    await performProjectScan(inputPath.value);
  });

  btnScan.addEventListener('click', async () => {
    const p = inputPath.value.trim();
    if (!p) {
      showWin95Alert('Input Required', 'Please enter or select a project directory first.', '⚠️');
      return;
    }
    await performProjectScan(p);
  });
}

async function performProjectScan(folderPath) {
  const statusBanner = document.getElementById('project-scan-status-banner');
  const sbStatus = document.getElementById('sb-status');
  sbStatus.textContent = 'Scanning project...';
  statusBanner.className = 'status-banner info';
  statusBanner.innerHTML = `Scanning directory <b>${folderPath}</b>...`;

  logActivity(`Initiating scan on project directory: ${folderPath}`);

  try {
    const result = await callRust('scan_project_dir', { path: folderPath });

    if (!result.is_valid_directory) {
      statusBanner.className = 'status-banner error';
      statusBanner.innerHTML = `<b>Scan Failed:</b> ${result.error_message || 'Invalid directory'}`;
      resetProjectCardBadges();
      sbStatus.textContent = 'Project scan error';
      return;
    }

    // 1. Folder build
    const badgeBuild = document.getElementById('badge-build-folder');
    const pathBuild = document.getElementById('path-build-folder');
    if (result.build_folder_found) {
      badgeBuild.className = 'badge badge-found';
      badgeBuild.textContent = '✔ FOUND';
      pathBuild.textContent = `Path: ${result.build_folder_path}`;
    } else {
      badgeBuild.className = 'badge badge-notfound';
      badgeBuild.textContent = '✖ NOT FOUND';
      pathBuild.textContent = 'Path: Not found in project root';
    }

    // 2. File build.xml
    const badgeXml = document.getElementById('badge-build-xml');
    const pathXml = document.getElementById('path-build-xml');
    const descXml = document.getElementById('desc-build-xml');
    if (result.build_xml_found) {
      badgeXml.className = 'badge badge-found';
      badgeXml.textContent = '✔ FOUND';
      pathXml.textContent = `Path: ${result.build_xml_path}`;
      descXml.textContent = `Ant Project: ${result.project_name || 'N/A'} (Default Target: ${result.default_target || 'N/A'}) - ${result.build_xml_location_note || ''}`;
    } else {
      badgeXml.className = 'badge badge-notfound';
      badgeXml.textContent = '✖ NOT FOUND';
      pathXml.textContent = 'Path: Not found inside build/ or root';
      descXml.textContent = 'No build.xml script detected.';
    }

    // 3. Folder filter
    const badgeFilter = document.getElementById('badge-filter-folder');
    const pathFilter = document.getElementById('path-filter-folder');
    const descFilter = document.getElementById('desc-filter-folder');
    if (result.filter_folder_found) {
      badgeFilter.className = 'badge badge-found';
      badgeFilter.textContent = '✔ FOUND';
      pathFilter.textContent = `Path: ${result.filter_folder_path}`;
      descFilter.textContent = `Contains ${result.filter_files.length} property file(s).`;
    } else {
      badgeFilter.className = 'badge badge-notfound';
      badgeFilter.textContent = '✖ NOT FOUND';
      pathFilter.textContent = 'Path: Not found in build/filter or filter';
      descFilter.textContent = 'No filter properties directory found.';
    }

    // 4. Filter files list
    const listFilter = document.getElementById('list-filter-files');
    listFilter.innerHTML = '';
    if (result.filter_files.length === 0) {
      listFilter.innerHTML = '<li class="empty-state">No filter properties files found.</li>';
    } else {
      result.filter_files.forEach((file) => {
        const li = document.createElement('li');
        li.innerHTML = `📄 <b>${file}</b>`;
        listFilter.appendChild(li);
      });
    }

    // 5. Ant targets table
    const tbodyTargets = document.getElementById('tbody-ant-targets');
    tbodyTargets.innerHTML = '';
    if (result.ant_targets.length === 0) {
      tbodyTargets.innerHTML = '<tr><td colspan="4" class="empty-state">No targets found in build.xml.</td></tr>';
    } else {
      result.ant_targets.forEach((tgt, idx) => {
        const isDefault = tgt === result.default_target;
        const tr = document.createElement('tr');
        tr.innerHTML = `
          <td>${idx + 1}</td>
          <td><b>${tgt}</b></td>
          <td><span class="badge ${isDefault ? 'badge-found' : 'badge-pending'}">${isDefault ? 'DEFAULT' : 'AVAILABLE'}</span></td>
          <td>Ant Target</td>
        `;
        tbodyTargets.appendChild(tr);
      });
    }

    // Overall banner
    const allFound = result.build_folder_found && result.build_xml_found && result.filter_folder_found;
    if (allFound) {
      statusBanner.className = 'status-banner success';
      statusBanner.innerHTML = `<b>Project Verified:</b> All 3 required components ('build', 'build.xml', 'filter') are present!`;
    } else {
      statusBanner.className = 'status-banner warning';
      statusBanner.innerHTML = `<b>Scan Finished:</b> Some components are missing. Inspect the cards below for details.`;
    }

    sbStatus.textContent = 'Project scanned';
    logActivity(`Scan complete for ${folderPath}. Targets: ${result.ant_targets.length}, Filter files: ${result.filter_files.length}`);
  } catch (err) {
    statusBanner.className = 'status-banner error';
    statusBanner.innerHTML = `<b>Error:</b> ${err}`;
    sbStatus.textContent = 'Scan error';
    logActivity(`Scan failed: ${err}`);
  }
}

function resetProjectCardBadges() {
  ['build-folder', 'build-xml', 'filter-folder'].forEach((id) => {
    const badge = document.getElementById(`badge-${id}`);
    if (badge) {
      badge.className = 'badge badge-pending';
      badge.textContent = 'NOT SCANNED';
    }
  });
}

// TAB 2: ENVIRONMENT PROBING
function setupEnvironmentTab() {
  const btnRunCheck = document.getElementById('btn-run-env-check');
  const selectCmdView = document.getElementById('select-cmd-view');

  btnRunCheck.addEventListener('click', async () => {
    await performEnvironmentCheck();
  });

  selectCmdView.addEventListener('change', () => {
    updateTerminalCmdView();
  });
}

async function performEnvironmentCheck() {
  const sbStatus = document.getElementById('sb-status');
  const btnRunCheck = document.getElementById('btn-run-env-check');
  const timestampEl = document.getElementById('env-check-timestamp');

  sbStatus.textContent = 'Probing environment via CMD...';
  btnRunCheck.disabled = true;
  btnRunCheck.textContent = '⏳ Executing CMD Probes...';

  logActivity('Running CMD commands: ant -version, where ant, java -version, where java, javac -version, where javac...');

  try {
    const env = await callRust('check_system_environment');
    currentEnvData = env;

    // 1. Ant Card
    const badgeAnt = document.getElementById('badge-env-ant');
    const valAntVer = document.getElementById('val-env-ant-version');
    const valAntHome = document.getElementById('val-env-ant-home');
    const valAntExecs = document.getElementById('val-env-ant-execs');

    if (env.ant_installed) {
      badgeAnt.className = 'badge badge-found';
      badgeAnt.textContent = 'INSTALLED';
      valAntVer.textContent = env.ant_version || 'Detected';
      valAntHome.textContent = env.ant_home || 'Not Set in %ANT_HOME%';
      valAntExecs.innerHTML = env.ant_executables.join('<br>') || 'None';
      document.getElementById('sb-ant').textContent = `Ant: ${env.ant_version?.split(' compiled')[0] || 'Installed'}`;
    } else {
      badgeAnt.className = 'badge badge-notfound';
      badgeAnt.textContent = 'NOT FOUND';
      valAntVer.textContent = 'Not installed or not in PATH';
      valAntHome.textContent = env.ant_home || 'Not Set';
      valAntExecs.textContent = 'None';
      document.getElementById('sb-ant').textContent = 'Ant: Not Installed';
    }

    // 2. Java Runtime Card
    const badgeJava = document.getElementById('badge-env-java');
    const valJavaVer = document.getElementById('val-env-java-version');
    const valJavaHome = document.getElementById('val-env-java-home');
    const valJavaExecs = document.getElementById('val-env-java-execs');

    if (env.java_installed) {
      badgeJava.className = 'badge badge-found';
      badgeJava.textContent = 'INSTALLED';
      valJavaVer.textContent = env.java_version || 'Detected';
      valJavaHome.textContent = env.java_home || 'Not Set in %JAVA_HOME%';
      valJavaExecs.innerHTML = env.java_executables.join('<br>') || 'None';
      document.getElementById('sb-jdk').textContent = `Java: ${env.java_version?.slice(0, 22) || 'Installed'}`;
    } else {
      badgeJava.className = 'badge badge-notfound';
      badgeJava.textContent = 'NOT FOUND';
      valJavaVer.textContent = 'Not installed or not in PATH';
      valJavaHome.textContent = env.java_home || 'Not Set';
      valJavaExecs.textContent = 'None';
      document.getElementById('sb-jdk').textContent = 'Java: Not Found';
    }

    // 3. Java Compiler Card
    const badgeJavac = document.getElementById('badge-env-javac');
    const valJavacVer = document.getElementById('val-env-javac-version');
    const valJavacExecs = document.getElementById('val-env-javac-execs');
    const valJavacStatus = document.getElementById('val-env-javac-status');

    if (env.javac_installed) {
      badgeJavac.className = 'badge badge-found';
      badgeJavac.textContent = 'INSTALLED';
      valJavacVer.textContent = env.javac_version || 'Detected';
      valJavacExecs.innerHTML = env.javac_executables.join('<br>') || 'None';
      valJavacStatus.textContent = 'Full JDK compiler available (can build Java projects)';
    } else {
      badgeJavac.className = 'badge badge-notfound';
      badgeJavac.textContent = 'NOT FOUND';
      valJavacVer.textContent = 'javac not found in PATH';
      valJavacExecs.textContent = 'None';
      valJavacStatus.textContent = 'Only JRE present or JDK not in PATH (javac missing)';
    }

    timestampEl.textContent = `Last checked: ${new Date().toLocaleTimeString()}`;
    updateTerminalCmdView();

    sbStatus.textContent = 'Environment checked';
    logActivity('Environment inspection complete.');
  } catch (err) {
    sbStatus.textContent = 'Check failed';
    logActivity(`Environment check error: ${err}`);
    showWin95Alert('Environment Check Error', `Failed to execute CMD commands: ${err}`, '❌');
  } finally {
    btnRunCheck.disabled = false;
    btnRunCheck.textContent = '⚡ Run Environment Check via CMD';
  }
}

function updateTerminalCmdView() {
  if (!currentEnvData) return;
  const select = document.getElementById('select-cmd-view');
  const terminal = document.getElementById('terminal-cmd-output');
  const badgeExit = document.getElementById('cmd-exit-code-badge');

  const key = select.value;
  let cmdRes = null;

  switch (key) {
    case 'ant_version':
      cmdRes = currentEnvData.ant_version_cmd;
      break;
    case 'where_ant':
      cmdRes = currentEnvData.where_ant_cmd;
      break;
    case 'echo_ant_home':
      cmdRes = currentEnvData.echo_ant_home_cmd;
      break;
    case 'java_version':
      cmdRes = currentEnvData.java_version_cmd;
      break;
    case 'where_java':
      cmdRes = currentEnvData.where_java_cmd;
      break;
    case 'echo_java_home':
      cmdRes = currentEnvData.echo_java_home_cmd;
      break;
    case 'javac_version':
      cmdRes = currentEnvData.javac_version_cmd;
      break;
    case 'where_javac':
      cmdRes = currentEnvData.where_javac_cmd;
      break;
  }

  if (cmdRes) {
    badgeExit.textContent = `Exit Code: ${cmdRes.exit_code} (${cmdRes.success ? 'SUCCESS' : 'FAILED'})`;
    badgeExit.style.color = cmdRes.success ? '#008000' : '#c00000';

    let displayStr = `C:\\Windows\\system32> ${cmdRes.command}\n\n`;
    if (cmdRes.stdout) {
      displayStr += `[STDOUT]:\n${cmdRes.stdout}\n\n`;
    }
    if (cmdRes.stderr) {
      displayStr += `[STDERR]:\n${cmdRes.stderr}\n\n`;
    }
    if (!cmdRes.stdout && !cmdRes.stderr) {
      displayStr += `(No output returned)\n`;
    }
    terminal.textContent = displayStr;
  }
}

// TAB 3: RUNTIME & MANAGER
async function setupRuntimeTab() {
  const btnExtractAnt = document.getElementById('btn-extract-ant');
  const btnTestAnt = document.getElementById('btn-test-active-ant');
  const btnTestJdk = document.getElementById('btn-test-active-jdk');

  btnExtractAnt.addEventListener('click', async () => {
    await performExtractAnt();
  });

  btnTestAnt.addEventListener('click', async () => {
    if (!selectedAnt) {
      showWin95Alert('Selection Required', 'Please select an Ant option first.', '⚠️');
      return;
    }
    if (!selectedAnt.executable_path) {
      showWin95Alert('Not Ready', 'Selected Ant is in a ZIP archive. Click "Extract & Setup Bundled Ant" first.', '⚠️');
      return;
    }
    await testAntBinary(selectedAnt.executable_path);
  });

  btnTestJdk.addEventListener('click', async () => {
    if (!selectedJdk) {
      showWin95Alert('Selection Required', 'Please select a JDK first.', '⚠️');
      return;
    }
    if (!selectedJdk.java_executable) {
      showWin95Alert('Installer Notice', `This JDK is an installer package (${selectedJdk.name}). Install it first or select an installed JDK.`, 'ℹ️');
      return;
    }
    await testJdkBinary(selectedJdk.java_executable);
  });

  await reloadAntOptions();
  await reloadJdkOptions();
}

async function reloadAntOptions() {
  const container = document.getElementById('ant-options-container');
  container.innerHTML = '<div class="empty-state">Scanning available Ant versions...</div>';

  try {
    const res = await callRust('get_available_ants');
    allAntOptions = res.available_options;
    container.innerHTML = '';

    if (allAntOptions.length === 0) {
      container.innerHTML = '<div class="empty-state">No Apache Ant installations or archives detected.</div>';
      return;
    }

    allAntOptions.forEach((ant, idx) => {
      const card = document.createElement('div');
      card.className = `runtime-option-card ${idx === 0 ? 'selected' : ''}`;
      card.setAttribute('data-id', ant.id);

      const isDefault = idx === 0;
      if (isDefault) selectedAnt = ant;

      card.innerHTML = `
        <input type="radio" name="ant-selection" class="runtime-radio" ${isDefault ? 'checked' : ''} />
        <div class="runtime-info">
          <div class="runtime-name-row">
            <span class="runtime-name">${ant.name}</span>
            <span class="badge ${ant.is_ready_to_use ? 'badge-found' : 'badge-pending'}">${ant.is_ready_to_use ? 'READY' : 'ZIP ARCHIVE'}</span>
          </div>
          <div class="runtime-path">Location: ${ant.root_path}</div>
          <div class="runtime-status-msg">${ant.status_message}</div>
        </div>
      `;

      card.addEventListener('click', () => {
        document.querySelectorAll('.ant-options-list .runtime-option-card').forEach((c) => c.classList.remove('selected'));
        card.classList.add('selected');
        card.querySelector('input[type="radio"]').checked = true;
        selectedAnt = ant;
        logActivity(`Selected Ant: ${ant.name}`);
        document.getElementById('sb-ant').textContent = `Ant: ${ant.version || 'Selected'}`;
      });

      container.appendChild(card);
    });

    // Toggle extract panel
    const hasZip = allAntOptions.some((a) => a.source_type === 'bundled_zip');
    const extractPanel = document.getElementById('panel-extract-ant');
    if (extractPanel) {
      extractPanel.style.display = hasZip ? 'flex' : 'none';
    }
  } catch (err) {
    container.innerHTML = `<div class="empty-state">Failed to load Ant options: ${err}</div>`;
  }
}

async function performExtractAnt() {
  const zipAnt = allAntOptions.find((a) => a.source_type === 'bundled_zip');
  const zipPath = zipAnt ? zipAnt.root_path : 'D:\\vinx_tools\\bach_helper\\libs\\apache-ant-1.10.18-bin.zip';

  logActivity(`Extracting Ant ZIP archive from ${zipPath}...`);
  const btn = document.getElementById('btn-extract-ant');
  btn.disabled = true;
  btn.textContent = '⏳ Extracting...';

  try {
    const res = await callRust('extract_ant', { zipPath });
    if (res.success) {
      logActivity(res.message);
      showWin95Alert('Ant Extracted Successfully', `Apache Ant is now ready for use in:<br><code>${res.extracted_dir}</code><br><br>Version: <b>${res.version || '1.10.18'}</b>`, '✔');
      await reloadAntOptions();
    } else {
      logActivity(`Extraction error: ${res.message}`);
      showWin95Alert('Extraction Failed', res.message, '❌');
    }
  } catch (err) {
    showWin95Alert('Error', `Extraction process failed: ${err}`, '❌');
  } finally {
    btn.disabled = false;
    btn.textContent = '📂 Extract & Setup Bundled Ant';
  }
}

async function testAntBinary(exePath) {
  const feedback = document.getElementById('test-ant-feedback');
  feedback.textContent = 'Testing executable...';
  logActivity(`Testing Ant executable: ${exePath}`);

  try {
    const res = await callRust('test_ant', { executablePath: exePath });
    if (res.success) {
      feedback.textContent = `✔ Verified: ${res.version_output.slice(0, 30)}...`;
      feedback.style.color = '#008000';
      showWin95Alert('Ant Verification Succeeded', `Executable is valid and functioning properly!<br><br>Output:<br><pre style="background:#000;color:#00ff00;padding:6px">${res.version_output}</pre>`, '✔');
    } else {
      feedback.textContent = '✖ Verification failed';
      feedback.style.color = '#c00000';
      showWin95Alert('Verification Failed', `Error: ${res.error_message || 'Executable did not respond properly.'}`, '❌');
    }
  } catch (err) {
    feedback.textContent = `Test error: ${err}`;
  }
}

async function reloadJdkOptions() {
  const container = document.getElementById('jdk-options-container');
  container.innerHTML = '<div class="empty-state">Scanning JDKs...</div>';

  try {
    const res = await callRust('get_available_jdks');
    allJdkOptions = res.jdks;
    container.innerHTML = '';

    if (allJdkOptions.length === 0) {
      container.innerHTML = '<div class="empty-state">No JDKs found in libs or Program Files.</div>';
      return;
    }

    allJdkOptions.forEach((jdk, idx) => {
      const card = document.createElement('div');
      const isDefault = (res.recommended_jdk_id && jdk.id === res.recommended_jdk_id) || idx === 0;
      if (isDefault) selectedJdk = jdk;

      card.className = `runtime-option-card ${isDefault ? 'selected' : ''}`;
      card.setAttribute('data-id', jdk.id);

      card.innerHTML = `
        <input type="radio" name="jdk-selection" class="runtime-radio" ${isDefault ? 'checked' : ''} />
        <div class="runtime-info">
          <div class="runtime-name-row">
            <span class="runtime-name">${jdk.name}</span>
            ${jdk.is_recommended ? '<span class="badge-recommended">⭐ RECOMMENDED (JDK 25)</span>' : ''}
            <span class="badge ${jdk.is_ready_to_use ? 'badge-found' : 'badge-pending'}">${jdk.is_ready_to_use ? 'READY' : 'INSTALLER'}</span>
          </div>
          <div class="runtime-path">Path: ${jdk.root_path}</div>
          <div class="runtime-status-msg">${jdk.status_message}</div>
        </div>
      `;

      card.addEventListener('click', () => {
        document.querySelectorAll('.jdk-options-list .runtime-option-card').forEach((c) => c.classList.remove('selected'));
        card.classList.add('selected');
        card.querySelector('input[type="radio"]').checked = true;
        selectedJdk = jdk;
        logActivity(`Selected JDK: ${jdk.name}`);
        document.getElementById('sb-jdk').textContent = `JDK: ${jdk.version_label || 'Selected'}`;
      });

      container.appendChild(card);
    });

    if (selectedJdk) {
      document.getElementById('sb-jdk').textContent = `JDK: ${selectedJdk.version_label}`;
    }
  } catch (err) {
    container.innerHTML = `<div class="empty-state">Failed to load JDKs: ${err}</div>`;
  }
}

async function testJdkBinary(exePath) {
  const feedback = document.getElementById('test-jdk-feedback');
  feedback.textContent = 'Testing Java executable...';
  logActivity(`Testing Java executable: ${exePath}`);

  try {
    const res = await callRust('test_java', { executablePath: exePath });
    if (res.success) {
      feedback.textContent = `✔ Verified: ${res.version_output.split('\n')[0]}`;
      feedback.style.color = '#008000';
      showWin95Alert('Java Runtime Verified', `Java executable is working properly!<br><br>Version info:<br><pre style="background:#000;color:#00ff00;padding:6px">${res.version_output}</pre>`, '✔');
    } else {
      feedback.textContent = '✖ Verification failed';
      feedback.style.color = '#c00000';
      showWin95Alert('Verification Failed', `Error: ${res.error_message || 'Executable did not respond properly.'}`, '❌');
    }
  } catch (err) {
    feedback.textContent = `Test error: ${err}`;
  }
}

// TAB 4: CMD TESTER
function setupTerminalTab() {
  const btnRunCustom = document.getElementById('btn-run-custom-cmd');
  const inputCmd = document.getElementById('input-custom-cmd');
  const terminal = document.getElementById('terminal-custom-cmd');
  const btnClearLog = document.getElementById('btn-clear-log');

  btnRunCustom.addEventListener('click', async () => {
    const cmd = inputCmd.value.trim();
    if (!cmd) return;

    terminal.textContent = `Running: cmd.exe /c ${cmd} ...`;
    logActivity(`Executing custom command: ${cmd}`);

    try {
      const res = await callRust('run_custom_cmd', { command: cmd });
      let output = `C:\\Windows\\system32> ${cmd}\n\n`;
      if (res.stdout) output += `[STDOUT]:\n${res.stdout}\n\n`;
      if (res.stderr) output += `[STDERR]:\n${res.stderr}\n\n`;
      output += `Exit Code: ${res.exit_code} (${res.success ? 'Success' : 'Failed'})\n`;
      terminal.textContent = output;
    } catch (err) {
      terminal.textContent = `Execution Error: ${err}`;
    }
  });

  inputCmd.addEventListener('keydown', (e) => {
    if (e.key === 'Enter') btnRunCustom.click();
  });

  btnClearLog.addEventListener('click', () => {
    const appLog = document.getElementById('terminal-app-log');
    if (appLog) appLog.textContent = 'Log cleared.\n';
  });
}

// Global UI controls
function setupGlobalControls() {
  // About dialog
  const aboutModal = document.getElementById('about-modal');
  const btnCloseAbout = document.getElementById('btn-close-about');
  const btnOkAbout = document.getElementById('btn-ok-about');
  const menuHelp = document.getElementById('menu-help');

  const openAbout = () => { aboutModal.style.display = 'flex'; };
  const closeAbout = () => { aboutModal.style.display = 'none'; };

  menuHelp.addEventListener('click', openAbout);
  btnCloseAbout.addEventListener('click', closeAbout);
  btnOkAbout.addEventListener('click', closeAbout);

  // Alert modal close buttons
  document.getElementById('btn-close-alert').addEventListener('click', closeWin95Alert);
  document.getElementById('btn-ok-alert').addEventListener('click', closeWin95Alert);

  // Start menu button
  document.getElementById('btn-start-menu').addEventListener('click', () => {
    openAbout();
  });

  // Menubar actions
  document.getElementById('menu-file').addEventListener('click', () => {
    document.querySelector('.win95-tabs li[data-tab="tab-project"]').click();
  });
  document.getElementById('menu-view').addEventListener('click', () => {
    document.querySelector('.win95-tabs li[data-tab="tab-env"]').click();
  });
  document.getElementById('menu-tools').addEventListener('click', () => {
    document.querySelector('.win95-tabs li[data-tab="tab-runtime"]').click();
  });
}

// Bootstrap
window.addEventListener('DOMContentLoaded', async () => {
  setupTabs();
  setupGlobalControls();
  setupProjectTab();
  setupEnvironmentTab();
  setupRuntimeTab();
  setupTerminalTab();

  logActivity('Batch Helper Windows 95 Edition initialized.');

  // Automatically run environment check on startup
  await performEnvironmentCheck();

  // Automatically scan the default project path if exists
  const initialPath = document.getElementById('project-path-input').value;
  if (initialPath) {
    await performProjectScan(initialPath);
  }
});
