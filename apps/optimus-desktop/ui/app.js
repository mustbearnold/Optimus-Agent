(() => {
  const THEME_KEY = 'optimus.ui.theme';
  const html = document.documentElement;
  const $ = (id) => document.getElementById(id);
  function applyTheme(pref) {
    const resolved = pref === 'light' ? 'light' : 'dark';
    html.setAttribute('data-theme', resolved);
    html.style.colorScheme = resolved;
    localStorage.setItem(THEME_KEY, resolved);
  }
  applyTheme(localStorage.getItem(THEME_KEY) || 'dark');
  function toggleTheme() {
    applyTheme(html.getAttribute('data-theme') === 'dark' ? 'light' : 'dark');
  }
  if ($('themeToggle')) $('themeToggle').onclick = toggleTheme;
  if ($('themeToggleSide')) $('themeToggleSide').onclick = toggleTheme;
  window.addEventListener('keydown', (e) => {
    if ((e.key === 'd' || e.key === 'D') && !e.metaKey && !e.ctrlKey && !e.altKey) {
      const t = e.target;
      if (t && (t.tagName === 'INPUT' || t.tagName === 'TEXTAREA')) return;
      e.preventDefault();
      toggleTheme();
    }
  });
  // Detect native vs HTTP chrome
  // optimus.localhost (custom protocol WebView) is ALWAYS native — never treat as HTTP.
  function isNativeDesktop() {
    try {
      if (window.__OPTIMUS_HTTP_MODE__ === true) return false;
      const host = (location.hostname || '').toLowerCase();
      if (host === 'optimus.localhost' || host.endsWith('.localhost')) return true;
      if ((host === '127.0.0.1' || host === 'localhost') && location.port) return false;
      return !!(window.ipc || (window.chrome && window.chrome.webview));
    } catch { return false; }
  }
  const isHttp = !isNativeDesktop();
  document.body.classList.add(isHttp ? 'http-mode' : 'native-chrome');
  function postDragNative() {
    // Lightest possible drag IPC (no await) — also triggers Rust DragWindow fast-path.
    try {
      const payload = JSON.stringify({ id: Date.now(), method: 'window_drag', params: {} });
      if (window.ipc && typeof window.ipc.postMessage === 'function') {
        window.ipc.postMessage(payload);
        return true;
      }
      if (window.chrome && window.chrome.webview && window.chrome.webview.postMessage) {
        window.chrome.webview.postMessage(payload);
        return true;
      }
      if (window.optimus && typeof window.optimus.windowDrag === 'function') {
        window.optimus.windowDrag();
        return true;
      }
    } catch (e) { console.warn('postDragNative', e); }
    return false;
  }
  function winFire(method) {
    try {
      if (method === 'window_drag') {
        postDragNative();
        return;
      }
      if (window.optimus) {
        if (typeof window.optimus.invoke === 'function') {
          window.optimus.invoke(method, {});
          return;
        }
        const map = {
          window_minimize: 'windowMinimize',
          window_maximize: 'windowMaximize',
          window_close: 'windowClose',
        };
        const fn = map[method];
        if (fn && typeof window.optimus[fn] === 'function') {
          window.optimus[fn]();
          return;
        }
      }
      if (window.chrome && window.chrome.webview && window.chrome.webview.postMessage) {
        window.chrome.webview.postMessage(JSON.stringify({ id: Date.now(), method: method, params: {} }));
      }
    } catch (e) { console.warn(method, e); }
  }
  if ($('winMin')) $('winMin').onclick = () => { winFire('window_minimize'); };
  if ($('winMax')) $('winMax').onclick = () => { winFire('window_maximize'); };
  if ($('winClose')) $('winClose').onclick = () => { winFire('window_close'); };
  // Titlebar drag — root cause of prior failure:
  // 1) drag_window requires LMB still down when UI thread runs UserEvent; WebView2 IPC is async.
  // 2) preventDefault + dual pointerdown/mousedown disrupted capture.
  // Fix: fire OS drag_window ASAP AND track screen deltas with set_outer_position (always works).
  function isDragExempt(t) {
    return !!(t && t.closest && t.closest('.no-drag,button,a,input,select,textarea,.tb-win,.tb-actions'));
  }
  function bindTitlebarDrag() {
    if (isHttp) return;
    const el = $('titlebar') || $('tbDrag');
    if (!el) return;
    let drag = null; // { ox, oy, sx, sy }
    let raf = 0;
    let pending = null;
    let moved = false;

    function flushPos() {
      raf = 0;
      if (!pending) return;
      const { x, y } = pending;
      pending = null;
      try {
        if (window.optimus && typeof window.optimus.windowSetOuterPosition === 'function') {
          // fire-and-forget; SetOuterPosition fast-path on Rust side
          window.optimus.windowSetOuterPosition(x, y);
        } else {
          const payload = JSON.stringify({
            id: Date.now(),
            method: 'window_set_outer_position',
            params: { x, y },
          });
          if (window.ipc && window.ipc.postMessage) window.ipc.postMessage(payload);
          else if (window.chrome && window.chrome.webview) window.chrome.webview.postMessage(payload);
        }
      } catch (e) { console.warn('setOuter', e); }
    }

    el.addEventListener('mousedown', (e) => {
      if (e.button !== 0) return;
      if (isDragExempt(e.target)) return;
      // Double-click → maximize (wry custom_titlebar pattern)
      if (e.detail === 2) {
        winFire('window_maximize');
        return;
      }
      // Do NOT preventDefault — breaks Win32 drag_window capture.
      moved = false;
      // Attempt OS caption drag immediately (works when UserEvent is timely).
      postDragNative();

      // Arm manual position drag (reliable under WebView2 async IPC).
      const sx = e.screenX;
      const sy = e.screenY;
      const startManual = (pos) => {
        if (!pos || typeof pos.x !== 'number') return;
        drag = { ox: pos.x, oy: pos.y, sx, sy };
      };
      // Prefer sync cache if we already know position; else async fetch then track.
      if (window.optimus && typeof window.optimus.windowOuterPosition === 'function') {
        Promise.resolve(window.optimus.windowOuterPosition())
          .then(startManual)
          .catch(() => {});
      }

      const onMove = (ev) => {
        if (ev.buttons != null && (ev.buttons & 1) === 0) {
          onUp();
          return;
        }
        if (!drag) return;
        const dx = ev.screenX - drag.sx;
        const dy = ev.screenY - drag.sy;
        if (!moved && Math.abs(dx) + Math.abs(dy) < 2) return;
        moved = true;
        pending = { x: drag.ox + dx, y: drag.oy + dy };
        if (!raf) raf = requestAnimationFrame(flushPos);
      };
      const onUp = () => {
        drag = null;
        window.removeEventListener('mousemove', onMove, true);
        window.removeEventListener('mouseup', onUp, true);
      };
      window.addEventListener('mousemove', onMove, true);
      window.addEventListener('mouseup', onUp, true);
    });
  }
  bindTitlebarDrag();

  const LAYOUT_KEY = 'optimus.ui.layout';
  const PINS_KEY = 'optimus.ui.pins';

  const PROJECTS_KEY = 'optimus.ui.projects';
  const SESSION_PROJ_KEY = 'optimus.ui.sessionProject';
  const PROJ_EXPANDED_KEY = 'optimus.ui.projectExpanded';
  const SIDEBAR_W_KEY = 'optimus.ui.sidebarW';

  function loadProjects() {
    try {
      const raw = localStorage.getItem(PROJECTS_KEY);
      const arr = raw ? JSON.parse(raw) : [];
      return Array.isArray(arr) ? arr.filter((p) => p && p.id && p.name) : [];
    } catch { return []; }
  }
  function saveProjects(list) {
    try { localStorage.setItem(PROJECTS_KEY, JSON.stringify(list)); } catch {}
  }
  function loadSessionProjects() {
    try {
      const raw = localStorage.getItem(SESSION_PROJ_KEY);
      const o = raw ? JSON.parse(raw) : {};
      return o && typeof o === 'object' ? o : {};
    } catch { return {}; }
  }
  function saveSessionProjects(map) {
    try { localStorage.setItem(SESSION_PROJ_KEY, JSON.stringify(map || {})); } catch {}
  }
  function loadProjectExpanded() {
    try {
      const raw = localStorage.getItem(PROJ_EXPANDED_KEY);
      const o = raw ? JSON.parse(raw) : {};
      return o && typeof o === 'object' ? o : {};
    } catch { return {}; }
  }
  function saveProjectExpanded(map) {
    try { localStorage.setItem(PROJ_EXPANDED_KEY, JSON.stringify(map || {})); } catch {}
  }

  const PINNED_PROJECTS_KEY = 'optimus.ui.pinnedProjects';
  function loadPinnedProjects() {
    try {
      const raw = localStorage.getItem(PINNED_PROJECTS_KEY);
      const arr = raw ? JSON.parse(raw) : [];
      return Array.isArray(arr) ? arr.map(String) : [];
    } catch { return []; }
  }
  function savePinnedProjects(list) {
    try { localStorage.setItem(PINNED_PROJECTS_KEY, JSON.stringify(list.map(String))); } catch {}
  }
  function togglePinnedProject(id, force) {
    id = String(id || '');
    if (!id || id === '__inbox') return;
    let list = loadPinnedProjects();
    const has = list.includes(id);
    const on = force === undefined ? !has : !!force;
    if (on && !has) list.push(id);
    if (!on) list = list.filter((x) => x !== id);
    savePinnedProjects(list);
    renderSessions();
  }
  async function openFolderPath(path) {
    if (!path) return;
    try {
      if (window.optimus && typeof window.optimus.openPath === 'function') {
        await window.optimus.openPath(path);
      } else {
        await postRaw('open_path', { path });
      }
    } catch (e) {
      console.warn('openPath', e);
      alert(e.message || String(e));
    }
  }
  async function deleteSessionById(id) {
    id = String(id || '');
    if (!id) return;
    if (!confirm('Delete this session permanently?')) return;
    try {
      if (window.optimus && typeof window.optimus.deleteSession === 'function') {
        await window.optimus.deleteSession(id);
      } else {
        await postRaw('delete_session', { id });
      }
    } catch (e) {
      alert(e.message || String(e));
      return;
    }
    // cleanup local maps
    savePins(loadPins().filter((p) => p !== id));
    const map = loadSessionProjects();
    delete map[id];
    saveSessionProjects(map);
    if (String(state.sessionId) === id) {
      state.sessionId = null;
      state.messages = [];
      setHeading('New session');
      renderMessages();
    }
    await refreshSessions();
  }
  async function renameSessionById(id, suggested) {
    id = String(id || '');
    if (!id) return;
    const current = (state.sessions || []).find((s) => String(s.id) === id);
    const prev = (suggested != null ? suggested : (current && current.title)) || 'session';
    const next = window.prompt('Rename session', prev);
    if (next == null) return; // cancelled
    const title = String(next).trim();
    if (!title) {
      alert('Title cannot be empty');
      return;
    }
    if (title === prev) return;
    try {
      let r;
      if (window.optimus && typeof window.optimus.renameSession === 'function') {
        r = await window.optimus.renameSession(id, title);
      } else {
        r = await postRaw('rename_session', { id, title });
      }
      const newTitle = (r && r.title) || title;
      // update local list cache
      state.sessions = (state.sessions || []).map((s) =>
        String(s.id) === id ? Object.assign({}, s, { title: newTitle }) : s
      );
      if (String(state.sessionId) === id) setHeading(newTitle);
      renderSessions();
      // ensure list refresh from store
      try { await refreshSessions(); } catch {}
    } catch (e) {
      alert(e.message || String(e));
    }
  }
  async function newSessionInProject(projectId) {
    const s = await newSession();
    if (s && s.id && projectId && projectId !== '__inbox') {
      assignSessionToProject(s.id, projectId);
      const exp = loadProjectExpanded();
      exp[projectId] = true;
      saveProjectExpanded(exp);
    }
    renderSessions();
    return s;
  }
  function reorderProjects(fromId, toId, place) {
    if (!fromId || fromId === '__inbox' || fromId === toId) return;
    let list = loadProjects();
    const fromIdx = list.findIndex((p) => p.id === fromId);
    if (fromIdx < 0) return;
    const [item] = list.splice(fromIdx, 1);
    if (!toId || toId === '__inbox') {
      list.push(item);
    } else {
      let toIdx = list.findIndex((p) => p.id === toId);
      if (toIdx < 0) list.push(item);
      else {
        if (place === 'after') toIdx += 1;
        list.splice(toIdx, 0, item);
      }
    }
    saveProjects(list);
    renderSessions();
  }
  function applyDrop(payload, target) {
    if (!payload || !target) return;
    const kind = payload.kind;
    const id = String(payload.id || '');
    if (!id) return;
    if (kind === 'session') {
      if (target.type === 'pin') {
        let pins = loadPins();
        if (!pins.includes(id)) { pins.push(id); savePins(pins); }
      } else if (target.type === 'unpin' || target.type === 'project') {
        // leaving pin zone
        if (target.type === 'project' || target.unpinSession) {
          savePins(loadPins().filter((p) => p !== id));
        }
        if (target.type === 'project') {
          assignSessionToProject(id, target.projectId === '__inbox' ? null : target.projectId);
          return; // assign already renders
        }
      } else if (target.type === 'session-list') {
        savePins(loadPins().filter((p) => p !== id));
      }
      renderSessions();
      return;
    }
    if (kind === 'project') {
      if (target.type === 'pin') {
        togglePinnedProject(id, true);
      } else if (target.type === 'unpin' || target.type === 'project-list') {
        togglePinnedProject(id, false);
        if (target.type === 'project' && target.projectId) {
          reorderProjects(id, target.projectId, target.place || 'before');
          return;
        }
        renderSessions();
      } else if (target.type === 'project' && target.projectId) {
        reorderProjects(id, target.projectId, target.place || 'before');
      }
    }
  }
  function parseDrag(ev) {
    try {
      const raw = ev.dataTransfer.getData('application/x-optimus') || ev.dataTransfer.getData('text/plain');
      return raw ? JSON.parse(raw) : null;
    } catch { return null; }
  }
  function setDrag(ev, payload) {
    const json = JSON.stringify(payload);
    ev.dataTransfer.setData('application/x-optimus', json);
    ev.dataTransfer.setData('text/plain', json);
    ev.dataTransfer.effectAllowed = 'move';
  }
  function bindDropTarget(el, targetFn) {
    if (!el || el.dataset.dropBound === '1') return;
    el.dataset.dropBound = '1';
    el.addEventListener('dragover', (e) => {
      e.preventDefault();
      e.dataTransfer.dropEffect = 'move';
      el.classList.add('drop-hover');
    });
    el.addEventListener('dragleave', () => el.classList.remove('drop-hover'));
    el.addEventListener('drop', (e) => {
      e.preventDefault();
      e.stopPropagation();
      el.classList.remove('drop-hover');
      const payload = parseDrag(e);
      const target = typeof targetFn === 'function' ? targetFn(e) : targetFn;
      applyDrop(payload, target);
    });
  }

  function toggleProjectExpanded(id) {
    const m = loadProjectExpanded();
    m[id] = !m[id];
    saveProjectExpanded(m);
    // Surgical DOM toggle — avoid full list thrash when possible
    const g = document.querySelector(`.proj-group[data-proj-id="${CSS.escape ? CSS.escape(id) : id}"]`);
    if (g) g.classList.toggle('open', !!m[id]);
    else renderSessions();
  }
  function assignSessionToProject(sessionId, projectId) {
    const map = loadSessionProjects();
    if (!projectId) delete map[String(sessionId)];
    else map[String(sessionId)] = projectId;
    saveSessionProjects(map);
    renderSessions();
  }
  function addProjectFromPath(path, name) {
    const list = loadProjects();
    const norm = String(path || '').replace(/\\/g, '/');
    if (!norm) return null;
    if (list.some((p) => String(p.path || '').replace(/\\/g, '/').toLowerCase() === norm.toLowerCase())) {
      return list.find((p) => String(p.path || '').replace(/\\/g, '/').toLowerCase() === norm.toLowerCase());
    }
    const base = name || norm.split('/').filter(Boolean).pop() || 'project';
    const proj = {
      id: 'p_' + Date.now().toString(36) + '_' + Math.random().toString(36).slice(2, 7),
      name: base,
      path: path,
    };
    list.push(proj);
    saveProjects(list);
    const exp = loadProjectExpanded();
    exp[proj.id] = true;
    saveProjectExpanded(exp);
    renderSessions();
    return proj;
  }
  async function addProjectDialog() {
    try {
      let r = null;
      if (window.optimus && typeof window.optimus.pickFolder === 'function') {
        r = await window.optimus.pickFolder();
      } else if (typeof postRaw === 'function') {
        r = await postRaw('pick_folder', {});
      }
      if (!r || r.cancelled) {
        // HTTP / cancel: allow manual path entry
        if (r && r.mode === 'http-stub') {
          const path = window.prompt('Project folder path (absolute):', 'E:\\\\Projects\\\\');
          if (!path) return;
          const name = path.replace(/[\\/]+$/, '').split(/[\\/]/).pop() || 'project';
          addProjectFromPath(path, name);
          return;
        }
        return;
      }
      addProjectFromPath(r.path, r.name);
    } catch (e) {
      console.warn('addProject', e);
      const path = window.prompt('Project folder path (absolute):', '');
      if (path) addProjectFromPath(path, path.split(/[\\\\/]/).pop());
    }
  }

  // Left sidebar horizontal resize
  (function bindLeftResize() {
    try {
      const w = localStorage.getItem(SIDEBAR_W_KEY);
      // Sanitize — corrupt values (0, empty, non-px) can collapse the grid to black void.
      if (w && /^-?\d+(\.\d+)?px$/.test(w.trim())) {
        const n = parseFloat(w);
        if (n >= 160 && n <= 640) {
          document.documentElement.style.setProperty('--sidebar-w', n + 'px');
        } else {
          localStorage.removeItem(SIDEBAR_W_KEY);
        }
      } else if (w) {
        localStorage.removeItem(SIDEBAR_W_KEY);
      }
    } catch {}
    const handle = () => $('leftResize');
    const el = handle();
    if (!el) return;
    let dragging = false;
    el.addEventListener('pointerdown', (e) => {
      e.preventDefault();
      e.stopPropagation();
      dragging = true;
      el.classList.add('dragging');
      const onMove = (ev) => {
        if (!dragging) return;
        const shell = document.querySelector('.shell') || document.body;
        const rect = shell.getBoundingClientRect();
        let w = ev.clientX - rect.left;
        w = Math.max(180, Math.min(Math.floor(rect.width * 0.5), w));
        document.documentElement.style.setProperty('--sidebar-w', w + 'px');
      };
      const onUp = () => {
        dragging = false;
        el.classList.remove('dragging');
        window.removeEventListener('pointermove', onMove);
        window.removeEventListener('pointerup', onUp);
        try {
          localStorage.setItem(SIDEBAR_W_KEY, getComputedStyle(document.documentElement).getPropertyValue('--sidebar-w').trim());
        } catch {}
      };
      window.addEventListener('pointermove', onMove);
      window.addEventListener('pointerup', onUp);
    });
  })();



  // --- Left rail: list mode, resize, context pin, settings ---
  const LIST_MODE_KEY = 'optimus.ui.listMode';
  const PINNED_H_KEY = 'optimus.ui.pinnedH';
  try {
    const ph = localStorage.getItem(PINNED_H_KEY);
    if (ph) document.documentElement.style.setProperty('--pinned-h', ph);
  } catch {}

  function setListMode(mode) {
    state.listMode = mode === 'sessions' ? 'sessions' : 'projects';
    try { localStorage.setItem(LIST_MODE_KEY, state.listMode); } catch {}
    const sp = $('sessionList');
    const pp = $('projectList');
    const lab = $('sessionsLabel');
    if (sp) sp.hidden = state.listMode !== 'sessions';
    if (pp) pp.hidden = state.listMode !== 'projects';
    if (lab) lab.textContent = state.listMode === 'sessions' ? 'Sessions' : 'Projects';
    document.querySelectorAll('#listModeToggle button').forEach((b) => {
      b.classList.toggle('active', b.dataset.mode === state.listMode);
    });
    renderSessions();
  }

  if ($('projectAdd')) $('projectAdd').onclick = (e) => { e.preventDefault(); e.stopPropagation(); addProjectDialog(); };

  if ($('modeProjects')) $('modeProjects').onclick = () => setListMode('projects');
  if ($('modeSessions')) $('modeSessions').onclick = () => setListMode('sessions');

  // Vertical resize between pinned and list
  (function bindRailResize() {
    const handle = $('railResize');
    const split = $('railSplit');
    if (!handle || !split) return;
    let dragging = false;
    handle.addEventListener('mousedown', (e) => {
      e.preventDefault();
      dragging = true;
      handle.classList.add('dragging');
      const onMove = (ev) => {
        if (!dragging) return;
        const rect = split.getBoundingClientRect();
        let pct = ((ev.clientY - rect.top) / rect.height) * 100;
        pct = Math.max(15, Math.min(70, pct));
        document.documentElement.style.setProperty('--pinned-h', pct + '%');
      };
      const onUp = () => {
        dragging = false;
        handle.classList.remove('dragging');
        window.removeEventListener('mousemove', onMove);
        window.removeEventListener('mouseup', onUp);
        try {
          localStorage.setItem(PINNED_H_KEY, getComputedStyle(document.documentElement).getPropertyValue('--pinned-h').trim());
        } catch {}
      };
      window.addEventListener('mousemove', onMove);
      window.addEventListener('mouseup', onUp);
    });
  })();

  // Settings popover
  if ($('settingsBtn') && $('settingsPop')) {
    $('settingsBtn').onclick = (e) => {
      e.stopPropagation();
      const pop = $('settingsPop');
      const open = !pop.classList.contains('open');
      pop.classList.toggle('open', open);
      pop.setAttribute('aria-hidden', open ? 'false' : 'true');
      if (open) refreshCron().catch(() => {});
    };
    document.addEventListener('click', (e) => {
      const pop = $('settingsPop');
      if (!pop || !pop.classList.contains('open')) return;
      if (pop.contains(e.target) || e.target === $('settingsBtn')) return;
      pop.classList.remove('open');
      pop.setAttribute('aria-hidden', 'true');
    });
  }

  // Context menu pin/unpin
  let _ctxSessionId = null;

  let _ctxProjectId = null;
  function hideProjectCtx() {
    const m = $('projectCtx');
    if (m) m.classList.remove('open');
    _ctxProjectId = null;
  }
  function showProjectCtx(id, x, y) {
    hideSessionCtx();
    const m = $('projectCtx');
    if (!m) return;
    _ctxProjectId = String(id);
    const pinned = loadPinnedProjects().includes(String(id));
    const pinBtn = m.querySelector('[data-act="pin-project"]');
    const unpinBtn = m.querySelector('[data-act="unpin-project"]');
    if (pinBtn) pinBtn.style.display = pinned ? 'none' : 'block';
    if (unpinBtn) unpinBtn.style.display = pinned ? 'block' : 'none';
    m.style.left = Math.min(x, window.innerWidth - 200) + 'px';
    m.style.top = Math.min(y, window.innerHeight - 180) + 'px';
    m.classList.add('open');
  }
  document.addEventListener('click', () => { hideProjectCtx(); });
  if ($('projectCtx')) {
    $('projectCtx').querySelectorAll('button[data-act]').forEach((btn) => {
      btn.addEventListener('click', async (e) => {
        e.stopPropagation();
        const act = btn.getAttribute('data-act');
        const id = _ctxProjectId;
        hideProjectCtx();
        if (!id) return;
        const proj = loadProjects().find((p) => p.id === id);
        if (act === 'open-folder') {
          if (proj && proj.path) await openFolderPath(proj.path);
          else alert('No folder path on this project');
        } else if (act === 'new-session') {
          await newSessionInProject(id);
        } else if (act === 'pin-project') {
          togglePinnedProject(id, true);
        } else if (act === 'unpin-project') {
          togglePinnedProject(id, false);
        } else if (act === 'remove-project') {
          if (!confirm('Remove project from list? Sessions stay in Inbox.')) return;
          saveProjects(loadProjects().filter((p) => p.id !== id));
          savePinnedProjects(loadPinnedProjects().filter((x) => x !== id));
          const map = loadSessionProjects();
          Object.keys(map).forEach((sid) => { if (map[sid] === id) delete map[sid]; });
          saveSessionProjects(map);
          renderSessions();
        }
      });
    });
  }

  function hideSessionCtx() {
    const m = $('sessionCtx');
    if (m) m.classList.remove('open');
    _ctxSessionId = null;
  }
  function showSessionCtx(id, x, y, pinned) {
    try { hideProjectCtx(); } catch {}
    const m = $('sessionCtx');
    if (!m) return;
    _ctxSessionId = String(id);
    const pinBtn = m.querySelector('[data-act="pin"]');
    const unpinBtn = m.querySelector('[data-act="unpin"]');
    if (pinBtn) pinBtn.style.display = pinned ? 'none' : 'block';
    if (unpinBtn) unpinBtn.style.display = pinned ? 'block' : 'none';
    m.style.left = Math.min(x, window.innerWidth - 180) + 'px';
    m.style.top = Math.min(y, window.innerHeight - 140) + 'px';
    m.classList.add('open');
    const wrap = m.querySelector('#ctxMoveWrap');
    if (wrap) {
      const projects = loadProjects();
      wrap.innerHTML = projects.length
        ? projects.map((p) => `<button type="button" data-act="move" data-proj="${esc(p.id)}">Move to ${esc(p.name)}</button>`).join('')
        : '<div class="cap-empty" style="padding:4px 12px;font-size:11px">Add a project with +</div>';
      wrap.querySelectorAll('button[data-act="move"]').forEach((b) => {
        b.onclick = (ev) => {
          ev.stopPropagation();
          const id = _ctxSessionId;
          const pid = b.getAttribute('data-proj');
          hideSessionCtx();
          if (id && pid) assignSessionToProject(id, pid);
        };
      });
    }
  }
  document.addEventListener('click', hideSessionCtx);
  document.addEventListener('contextmenu', (e) => {
    // allow only on our menu
    if (e.target.closest && e.target.closest('#sessionCtx')) return;
  });
  if ($('sessionCtx')) {
    $('sessionCtx').querySelectorAll('button[data-act]').forEach((btn) => {
      btn.addEventListener('click', async (e) => {
        e.stopPropagation();
        const act = btn.getAttribute('data-act');
        const id = _ctxSessionId;
        hideSessionCtx();
        if (!id) return;
        if (act === 'pin') {
          let pins = loadPins();
          if (!pins.includes(id)) { pins.push(id); savePins(pins); renderSessions(); }
        } else if (act === 'unpin') {
          savePins(loadPins().filter((p) => p !== id));
          renderSessions();
        } else if (act === 'open') {
          openSession(id);
        } else if (act === 'copy') {
          try { await navigator.clipboard.writeText(id); } catch {}
        } else if (act === 'unassign') {
          assignSessionToProject(id, null);
        } else if (act === 'rename') {
          await renameSessionById(id);
        } else if (act === 'delete') {
          await deleteSessionById(id);
        }
      });
    });
  }

  window.addEventListener('keydown', (e) => {
    if ((e.ctrlKey || e.metaKey) && (e.key === 'n' || e.key === 'N')) {
      const t = e.target;
      if (t && (t.tagName === 'INPUT' || t.tagName === 'TEXTAREA')) return;
      e.preventDefault();
      if ($('newThread')) $('newThread').click();
    }
    // F2 rename focused session row or active session
    if (e.key === 'F2') {
      const t = e.target;
      if (t && (t.tagName === 'INPUT' || t.tagName === 'TEXTAREA' || t.isContentEditable)) return;
      e.preventDefault();
      const focused = document.activeElement && document.activeElement.closest
        ? document.activeElement.closest('.thread[data-id]')
        : null;
      const id = (focused && focused.dataset.id) || state.sessionId;
      if (id) renameSessionById(id);
    }
  });


  const state = {
    sessionId: null,
    sessions: [],
    messages: [],
    busy: false,
    activeCancel: null,
    cancelRequested: false,
    filter: '',
    booted: false,
    tasks: [],
    streamRaf: 0,
    stickScroll: true,
    route: 'chat',
    layout: { right: false, term: false, logs: false },
    filesPath: '',
    filesLoading: false,
    filePreviewPath: '',
    sessionStartedEpochMs: Date.now(),
    turnStartedPerfMs: null,
    turnTimerHandle: null,
    latestTurnTimings: null,
    listMode: (function(){ try { return localStorage.getItem('optimus.ui.listMode') || 'projects'; } catch { return 'projects'; } })(),
  };
  function formatDuration(ms) {
    const value = Math.max(0, Number(ms) || 0);
    if (value < 1000) return `${Math.round(value)} ms`;
    if (value < 60000) return `${(value / 1000).toFixed(value < 10000 ? 1 : 0)} s`;
    const minutes = Math.floor(value / 60000);
    const seconds = Math.floor((value % 60000) / 1000);
    return `${minutes}m ${String(seconds).padStart(2, '0')}s`;
  }
  function parseSessionEpoch(value) {
    if (typeof value !== 'string') return Date.now();
    if (value.startsWith('ts:')) {
      const seconds = Number(value.slice(3));
      if (Number.isFinite(seconds)) return seconds * 1000;
    }
    const parsed = Date.parse(value);
    return Number.isFinite(parsed) ? parsed : Date.now();
  }
  function setSessionClock(createdAt) {
    state.sessionStartedEpochMs = createdAt ? parseSessionEpoch(createdAt) : Date.now();
    paintSessionClock();
  }
  function paintSessionClock() {
    const node = $('sessionTimer');
    if (node) node.textContent = `session ${formatDuration(Date.now() - state.sessionStartedEpochMs)}`;
  }
  function paintTurnClock() {
    const node = $('turnTimer');
    if (!node || state.turnStartedPerfMs == null) return;
    node.textContent = `turn ${formatDuration(performance.now() - state.turnStartedPerfMs)}`;
  }
  function startTurnClock() {
    state.turnStartedPerfMs = performance.now();
    state.latestTurnTimings = null;
    const node = $('turnTimer');
    if (node) node.dataset.active = 'true';
    paintTurnClock();
    if (state.turnTimerHandle) clearInterval(state.turnTimerHandle);
    state.turnTimerHandle = setInterval(paintTurnClock, 250);
  }
  function stopTurnClock(totalMs) {
    if (state.turnTimerHandle) clearInterval(state.turnTimerHandle);
    state.turnTimerHandle = null;
    const node = $('turnTimer');
    if (node) {
      const measured = totalMs == null && state.turnStartedPerfMs != null
        ? performance.now() - state.turnStartedPerfMs
        : totalMs;
      node.textContent = `turn ${formatDuration(measured || 0)}`;
      node.dataset.active = 'false';
    }
    state.turnStartedPerfMs = null;
  }
  setInterval(paintSessionClock, 1000);
  paintSessionClock();
  function loadLayout() {
    try {
      const raw = localStorage.getItem(LAYOUT_KEY);
      if (!raw) return;
      const j = JSON.parse(raw);
      if (typeof j.right === 'boolean') state.layout.right = j.right;
      if (typeof j.term === 'boolean') state.layout.term = j.term;
      if (typeof j.logs === 'boolean') state.layout.logs = j.logs;
    } catch {}
  }
  function saveLayout() {
    try {
      localStorage.setItem(LAYOUT_KEY, JSON.stringify({
        right: !!state.layout.right,
        term: !!state.layout.term,
        logs: !!state.layout.logs,
      }));
    } catch {}
  }
  function applyLayout() {
    const right = $('rightPane');
    const term = $('termPane');
    const logs = $('logsDrawer');
    const row = $('appRow');
    const tr = $('toggleRight');
    const tt = $('toggleTerm');
    const tl = $('toggleLogs');
    if (right) {
      if (state.layout.right) { right.classList.add('open'); right.removeAttribute('hidden'); }
      else { right.classList.remove('open'); right.setAttribute('hidden', ''); }
    }
    if (term) {
      if (state.layout.term) { term.classList.add('open'); term.removeAttribute('hidden'); }
      else { term.classList.remove('open'); term.setAttribute('hidden', ''); }
    }
    if (logs) {
      if (state.layout.logs) { logs.classList.add('open'); logs.removeAttribute('hidden'); }
      else { logs.classList.remove('open'); logs.setAttribute('hidden', ''); }
    }
    if (row) row.classList.toggle('right-open', !!state.layout.right);
    if (tr) { tr.classList.toggle('active', !!state.layout.right); tr.setAttribute('aria-pressed', state.layout.right ? 'true' : 'false'); }
    if (tt) { tt.classList.toggle('active', !!state.layout.term); tt.setAttribute('aria-pressed', state.layout.term ? 'true' : 'false'); }
    if (tl) { tl.classList.toggle('active', !!state.layout.logs); tl.setAttribute('aria-pressed', state.layout.logs ? 'true' : 'false'); }
  }
  function setRoute(route) {
    const r = ['chat', 'capabilities', 'messaging', 'artifacts'].includes(route) ? route : 'chat';
    state.route = r;
    document.querySelectorAll('.page').forEach((p) => {
      const on = p.dataset.page === r || p.id === ('page-' + r);
      p.classList.toggle('active', on);
      if (on) p.removeAttribute('hidden');
      else p.setAttribute('hidden', '');
    });
    document.querySelectorAll('#navPrimary .nav-item[data-route]').forEach((btn) => {
      if (btn.id === 'newThread') {
        btn.classList.toggle('active', r === 'chat');
      } else {
        btn.classList.toggle('active', btn.dataset.route === r);
      }
    });
    if (r === 'capabilities') {
      try { refreshCapabilitiesPage(); } catch (e) {}
    }
  }
  function setHeading(title) {
    const t = title || 'session';
    if ($('heading')) $('heading').textContent = t;
  }
  function esc(s) {
    return String(s ?? '').replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;');
  }
  /** True if content is serialized tool-call JSON (kernel intermediate turns). */
  function isToolCallJson(text) {
    const t = String(text ?? '').trim();
    if (!t.startsWith('[')) return false;
    try {
      const v = JSON.parse(t);
      return Array.isArray(v) && v.length > 0 && v.every(x => x && typeof x === 'object' && (x.name || x.id) && (x.arguments !== undefined || x.name));
    } catch {
      // Partial stream of tool JSON only — no trailing prose
      return /^\s*\[\s*\{\s*"id"\s*:\s*"call_[\s\S]*$/.test(t) && !/\n\s*[A-Za-z*]/.test(t.slice(t.indexOf(']')));
    }
  }
  /** Strip leaked tool-call JSON blobs from assistant text. */
  function stripToolCallNoise(text) {
    let t = String(text ?? '');
    if (isToolCallJson(t)) return '';
    // Remove complete tool-call JSON arrays (balanced-ish: from [{ "id":"call_  to matching ])
    t = t.replace(/\[\s*\{\s*"id"\s*:\s*"call_[^"]*"[\s\S]*?\}\s*\]/g, (m) => {
      try {
        const v = JSON.parse(m);
        if (Array.isArray(v) && v.every(x => x && x.name)) return '';
      } catch {}
      return m;
    });
    // Drop leading incomplete tool JSON if followed by prose
    t = t.replace(/^\s*\[\s*\{\s*"id"\s*:\s*"call_[\s\S]*?\]\s*/g, (m) => {
      try { JSON.parse(m); return ''; } catch { return m; }
    });
    return t.trim();
  }
  function prettyUrl(href) {
    try {
      const u = new URL(href);
      if (u.hostname.includes('news.google.com')) return 'Google News';
      if (u.hostname.includes('duckduckgo.com')) return 'DuckDuckGo';
      return u.hostname.replace(/^www\./, '') + (u.pathname.length > 1 ? u.pathname.slice(0, 28) + (u.pathname.length > 28 ? '…' : '') : '');
    } catch {
      return href.slice(0, 40) + (href.length > 40 ? '…' : '');
    }
  }
  function formatInline(escText) {
    // links: [label](url) then bare urls
    let s = escText;
    s = s.replace(/\[([^\]]+)\]\((https?:\/\/[^)\s]+)\)/g, (_, lab, href) => {
      const label = lab === 'source' || lab === 'Source' ? prettyUrl(href) : lab;
      return `<a class="md-link" href="${href}" title="${href}" target="_blank" rel="noreferrer">${label}</a>`;
    });
    s = s.replace(/(^|[\s(])(https?:\/\/[^\s)<]+)/g, (_, pre, href) => {
      return `${pre}<a class="md-link" href="${href}" title="${href}" target="_blank" rel="noreferrer">${prettyUrl(href)}</a>`;
    });
    // bold / italic (order matters)
    s = s.replace(/\*\*([^*]+)\*\*/g, '<strong>$1</strong>');
    s = s.replace(/(^|[^*])\*([^*]+)\*(?!\*)/g, '$1<em>$2</em>');
    s = s.replace(/`([^`\n]+)`/g, '<code class="inline-code">$1</code>');
    return s;
  }
  function formatRich(text) {
    const raw = stripToolCallNoise(text);
    if (!raw) return '';
    // Fenced code blocks first
    const parts = raw.split(/```/);
    let html = '';
    for (let i = 0; i < parts.length; i++) {
      if (i % 2 === 1) {
        const block = parts[i];
        const nl = block.indexOf('\n');
        let lang = '';
        let code = block;
        if (nl >= 0) {
          lang = block.slice(0, nl).trim();
          code = block.slice(nl + 1);
        }
        html += `<div class="code-block"><header><span>${esc(lang || 'code')}</span></header><pre><code>${esc(code.replace(/\n$/, ''))}</code></pre></div>`;
      } else {
        const chunk = parts[i];
        // paragraphs / lists
        const lines = chunk.split(/\n/);
        let listOpen = null; // 'ul' | 'ol'
        const closeList = () => {
          if (listOpen) { html += listOpen === 'ul' ? '</ul>' : '</ol>'; listOpen = null; }
        };
        let para = [];
        const flushPara = () => {
          if (!para.length) return;
          const body = formatInline(esc(para.join('\n'))).replace(/\n/g, '<br>');
          html += `<p class="md-p">${body}</p>`;
          para = [];
        };
        for (const line of lines) {
          const bullet = line.match(/^\s*[-*]\s+(.+)$/);
          const numbered = line.match(/^\s*(\d+)\.\s+(.+)$/);
          const heading = line.match(/^\s{0,3}(#{1,3})\s+(.+)$/);
          if (heading) {
            flushPara(); closeList();
            const lvl = heading[1].length;
            html += `<div class="md-h md-h${lvl}">${formatInline(esc(heading[2]))}</div>`;
          } else if (bullet) {
            flushPara();
            if (listOpen !== 'ul') { closeList(); html += '<ul class="md-ul">'; listOpen = 'ul'; }
            html += `<li>${formatInline(esc(bullet[1]))}</li>`;
          } else if (numbered) {
            flushPara();
            if (listOpen !== 'ol') { closeList(); html += '<ol class="md-ol">'; listOpen = 'ol'; }
            html += `<li>${formatInline(esc(numbered[2]))}</li>`;
          } else if (line.trim() === '') {
            flushPara(); closeList();
          } else {
            closeList();
            para.push(line);
          }
        }
        flushPara();
        closeList();
      }
    }
    return html;
  }
  function coalesceTools(tools) {
    if (!tools || !tools.length) return [];
    const order = [];
    const map = new Map();
    for (const t of tools) {
      const name = t.name || 'tool';
      let g = map.get(name);
      if (!g) {
        g = {
          name,
          status: t.status || 'ok',
          count: 0,
          runs: [],
          durationMs: 0,
        };
        map.set(name, g);
        order.push(g);
      }
      g.count += 1;
      if (t.status === 'run') g.status = 'run';
      else if (g.status !== 'run' && t.status === 'ok') g.status = 'ok';
      const d = String(t.detail || '').trim();
      if (d) g.runs.push(d);
      g.durationMs += Number(t.durationMs) || 0;
    }
    return order;
  }
  window.__optimusTest = Object.freeze({ coalesceTools, stripToolCallNoise });
  function formatToolDetail(name, runs) {
    const titles = [];
    const queries = [];
    let totalHits = 0;
    for (const detail of runs) {
      try {
        const arrow = detail.indexOf('->');
        const jsonPart = arrow >= 0 ? detail.slice(arrow + 2).trim() : detail.trim();
        const v = JSON.parse(jsonPart);
        if (v && typeof v.query === 'string') queries.push(v.query);
        if (v && Array.isArray(v.results)) {
          totalHits += v.count ?? v.results.length;
          for (const r of v.results.slice(0, 3)) {
            const line = `${r.title || 'result'}${r.url ? ' — ' + prettyUrl(r.url) : ''}`;
            if (line && !titles.includes(line)) titles.push(line);
          }
        } else if (v && v.query) {
          // no results payload
        } else {
          if (detail && !titles.includes(detail.slice(0, 120))) titles.push(detail.slice(0, 160));
        }
      } catch {
        // plain query string / short detail
        if (/^query:/i.test(detail) || detail.length < 160) {
          if (!queries.includes(detail)) queries.push(detail);
        } else if (detail) {
          titles.push(detail.slice(0, 160));
        }
      }
    }
    const parts = [];
    if (runs.length > 1) parts.push(`${runs.length} calls`);
    if (totalHits) parts.push(`${totalHits} hits`);
    if (queries.length) {
      const qshow = queries.slice(0, 3).map(q => q.length > 70 ? q.slice(0, 70) + '…' : q);
      parts.push('queries:\n  · ' + qshow.join('\n  · '));
    }
    if (titles.length) {
      parts.push('top:\n  · ' + titles.slice(0, 6).join('\n  · '));
    }
    if (!parts.length) return name;
    return parts.join('\n');
  }
  function toolsHtml(tools) {
    const groups = coalesceTools(tools);
    if (!groups.length) return '';
    return groups.map(g => {
      const st = g.status === 'ok' ? 'ok' : (g.status === 'run' ? 'run' : '');
      const label = g.count > 1 ? `${g.name} ×${g.count}` : g.name;
      const detail = formatToolDetail(g.name, g.runs);
      return `<details class="tool-card" ${g.status === 'run' ? 'open' : ''}>
        <summary><span class="tool-dot ${st}"></span><span>${esc(label)}</span>
        <span style="margin-left:auto;color:var(--text-3);font-size:11px">${g.durationMs ? esc(formatDuration(g.durationMs)) + ' · ' : ''}${esc(g.status || '')}${g.count > 1 ? ` · ${g.count}` : ''}</span></summary>
        <pre>${esc(detail)}</pre>
      </details>`;
    }).join('');
  }
  function nearBottom(el, px) {
    return (el.scrollHeight - el.scrollTop - el.clientHeight) < (px || 96);
  }
  function stickToBottom(force) {
    const el = $('chat');
    if (!el) return;
    if (force || state.stickScroll || nearBottom(el, 120)) {
      el.scrollTop = el.scrollHeight;
    }
  }
  function setTaskPanel(open) {
    const p = $('taskPanel');
    const chip = $('taskChip');
    if (!p) return;
    p.classList.toggle('open', !!open);
    p.setAttribute('aria-hidden', open ? 'false' : 'true');
    if (chip) chip.classList.toggle('active', !!open);
  }
  function upsertTask(name, detail, status, callId) {
    const key = name || 'tool';
    let t = state.tasks.find(x => x.name === key);
    if (!t) {
      t = { id: Date.now() + Math.random(), name: key, detail: detail || '', status: status || 'run', count: 1, lastDetail: detail || '', callIds: callId ? [callId] : [], suppressedCount: 0 };
      state.tasks.push(t);
    } else {
      if (callId && !t.callIds.includes(callId)) {
        t.callIds.push(callId);
        t.count = (t.count || 1) + 1;
      } else if (!callId && status === 'run' && detail && detail !== t.lastDetail) {
        t.count = (t.count || 1) + 1;
        t.lastDetail = detail;
      }
      if (detail) t.detail = t.count > 1 ? `${t.count}× ${key}` : detail;
      if (status) t.status = status;
    }
    renderTasks();
    if (status === 'run') setTaskPanel(true);
  }
  function renderTasks() {
    const body = $('taskBody');
    const count = $('taskCount');
    const n = state.tasks.length;
    if (count) count.textContent = String(n);
    if (!body) return;
    if (!n) {
      body.innerHTML = '<div class="task-empty">No tools running</div>';
      return;
    }
    body.innerHTML = state.tasks.map(t => `
      <div class="task-item">
        <span class="tool-dot ${t.status === 'ok' ? 'ok' : (t.status === 'run' ? 'run' : '')}"></span>
        <div>
          <div class="t-name">${esc(t.name)}${t.count > 1 ? ` ×${t.count}` : ''}${t.suppressedCount ? ` · ${t.suppressedCount} suppressed` : ''}</div>
          <div class="t-detail">${t.durationMs != null ? `${esc(formatDuration(t.durationMs))} · ` : ''}${esc(t.detail || t.status || '')}</div>
        </div>
      </div>`).join('');
  }
  function finishTasks() {
    state.tasks = state.tasks.map(t => t.status === 'run' ? { ...t, status: 'ok' } : t);
    renderTasks();
  }
  function msgHtml(m, idx, enter) {
    const enterCls = enter ? ' is-enter' : '';
    if (m.role === 'user') {
      return `<div class="msg user${enterCls}" data-msg-idx="${idx}"><div class="bubble">${esc(m.content)}</div></div>`;
    }
    const tools = toolsHtml(m.tools);
    let body;
    if (!m.content && m.streaming) {
      body = `<span class="typing" id="typing">Optimus is working…</span>`;
    } else {
      body = `<div class="bubble-body" data-stream-body="1">${formatRich(m.content || '')}</div>`;
      if (m.streaming && m.content) body += `<span class="stream-cursor" aria-hidden="true"></span>`;
    }
    return `<div class="msg assistant${enterCls}" data-msg-idx="${idx}" data-streaming="${m.streaming ? '1' : '0'}">
      <div class="bubble">${tools}${body}</div>
      ${m.meta ? `<div class="status-strip">${m.meta}</div>` : ''}
    </div>`;
  }
  function renderMessages(opts) {
    const o = opts || {};
    const root = $('messages');
    if (!state.messages.length) {
      root.innerHTML = `<div class="empty" id="emptyState">
        <h2>${state.sessionId ? 'Empty session' : 'Start a session'}</h2>
        <p>Messages persist in sessions.db under your Optimus home.</p>
      </div>`;
      return;
    }
    const enterFrom = typeof o.enterFrom === 'number' ? o.enterFrom : -1;
    root.innerHTML = state.messages.map((m, i) => msgHtml(m, i, i >= enterFrom && enterFrom >= 0)).join('');
    // strip enter class after paint so later rebuilds don't re-animate
    requestAnimationFrame(() => {
      root.querySelectorAll('.msg.is-enter').forEach(n => n.classList.remove('is-enter'));
      stickToBottom(!!o.forceScroll);
    });
  }
  /** 100fps-friendly stream patch: mutate only the live assistant bubble. */
  function scheduleStreamPaint(idx) {
    if (state.streamRaf) return;
    state.streamRaf = requestAnimationFrame(() => {
      state.streamRaf = 0;
      paintStreamBubble(idx);
    });
  }
  function paintStreamBubble(idx) {
    const m = state.messages[idx];
    if (!m) return;
    let node = document.querySelector(`[data-msg-idx="${idx}"]`);
    if (!node) {
      renderMessages({ forceScroll: true });
      return;
    }
    const bubble = node.querySelector('.bubble');
    if (!bubble) {
      renderMessages({ forceScroll: true });
      return;
    }
    const tools = toolsHtml(m.tools);
    let body;
    if (!m.content && m.streaming) {
      body = `<span class="typing" id="typing">Optimus is working…</span>`;
    } else {
      body = `<div class="bubble-body" data-stream-body="1">${formatRich(m.content || '')}</div>`;
      if (m.streaming && m.content) body += `<span class="stream-cursor" aria-hidden="true"></span>`;
    }
    // Avoid full list rebuild — only this bubble
    bubble.innerHTML = tools + body;
    if (m.meta) {
      let strip = node.querySelector('.status-strip');
      if (!strip) {
        strip = document.createElement('div');
        strip.className = 'status-strip';
        node.appendChild(strip);
      }
      strip.innerHTML = m.meta;
    }
    stickToBottom(false);
  }
  function hasNative() {
    if (typeof window.__optimusHasNative === 'function' && window.__optimusHasNative()) return true;
    return !!(window.optimus && (
      window.ipc ||
      (window.chrome && window.chrome.webview)
    ));
  }
  async function api(fn, ...args) {
    if (!hasNative()) throw new Error('Not running inside optimus-desktop');
    return window.optimus[fn](...args);
  }
  function setAuthBanner(auth) {
    const el = $('authBanner');
    if (!el) return;
    // Happy-path auth is silent in the left rail (no "Codex ready · …" strip).
    // Only show problems; surface OK state on the status bar if present.
    const st = $('stModel');
    if (!auth) {
      el.hidden = false;
      el.className = 'auth-banner err';
      el.textContent = 'Native bridge offline — launch optimus-desktop.exe';
      return;
    }
    if (auth.present && !auth.access_expiring) {
      el.hidden = true;
      el.className = 'auth-banner ok';
      el.textContent = '';
      el.style.maxHeight = '';
      // Optional quiet status-bar hint (non-blocking)
      if ($('stHome') && auth.mode) {
        try {
          const h = $('stHome');
          if (h && !h.dataset.authOk) {
            h.title = `Codex ${auth.mode}${auth.has_refresh ? ' · refresh ok' : ''}`;
            h.dataset.authOk = '1';
          }
        } catch {}
      }
    } else if (auth.present && auth.access_expiring) {
      el.hidden = false;
      el.className = 'auth-banner warn';
      el.innerHTML = `<strong>Codex token expiring</strong> · will refresh on next chat`;
    } else {
      el.hidden = false;
      el.className = 'auth-banner err';
      el.innerHTML = `<strong>No Codex credentials</strong> · Settings → Import Codex`;
    }
  }

  function loadPins() {
    try {
      const raw = localStorage.getItem(PINS_KEY);
      const arr = raw ? JSON.parse(raw) : [];
      return Array.isArray(arr) ? arr.map(String) : [];
    } catch {
      return [];
    }
  }
  function savePins(pins) {
    try { localStorage.setItem(PINS_KEY, JSON.stringify(pins.map(String))); } catch {}
  }
  function togglePin(id, ev) {
    if (ev) { ev.preventDefault(); ev.stopPropagation(); }
    id = String(id || '');
    if (!id) return;
    let pins = loadPins();
    if (pins.includes(id)) pins = pins.filter((p) => p !== id);
    else pins.push(id);
    savePins(pins);
    renderSessions();
  }

  function threadRowHtml(s, pinned, projectId) {
    const active = s.id === state.sessionId ? 'active' : '';
    const pinSet = new Set(loadPins());
    const isPinned = pinned || pinSet.has(String(s.id));
    const pid = projectId != null ? projectId : (loadSessionProjects()[String(s.id)] || '');
    return `<div class="thread ${active}" draggable="true" data-id="${esc(s.id)}" data-pinned="${isPinned ? '1' : '0'}" data-project="${esc(pid || '')}" role="button" tabindex="0">
        <span class="thread-dot ${active ? 'on' : ''}" aria-hidden="true"></span>
        <div class="thread-title" title="${esc(s.title || 'session')}">${esc(s.title || 'session')}</div>
      </div>`;
  }
  function bindThreadList(box) {
    if (!box) return;
    box.querySelectorAll('.thread').forEach((row) => {
      row.addEventListener('click', (e) => {
        if (e.target.closest && e.target.closest('.thread-pin,.proj-new')) return;
        if (row.classList.contains('dragging')) return;
        openSession(row.dataset.id);
      });
      row.addEventListener('keydown', (e) => {
        if (e.key === 'Enter' || e.key === ' ') {
          e.preventDefault();
          openSession(row.dataset.id);
        }
        if (e.key === 'F2') {
          e.preventDefault();
          renameSessionById(row.dataset.id);
        }
      });
      row.addEventListener('dblclick', (e) => {
        e.preventDefault();
        e.stopPropagation();
        renameSessionById(row.dataset.id);
      });
      row.addEventListener('contextmenu', (e) => {
        e.preventDefault();
        e.stopPropagation();
        const pinned = row.dataset.pinned === '1';
        showSessionCtx(row.dataset.id, e.clientX, e.clientY, pinned);
      });
      row.addEventListener('dragstart', (e) => {
        row.classList.add('dragging');
        setDrag(e, { kind: 'session', id: row.dataset.id, projectId: row.dataset.project || '' });
      });
      row.addEventListener('dragend', () => row.classList.remove('dragging'));
    });
  }
  function bindProjectChrome(box) {
    if (!box) return;
    box.querySelectorAll('[data-proj-toggle]').forEach((btn) => {
      btn.onclick = (e) => {
        if (e.target.closest && e.target.closest('.proj-new')) return;
        e.preventDefault();
        e.stopPropagation();
        toggleProjectExpanded(btn.getAttribute('data-proj-toggle'));
      };
      btn.addEventListener('contextmenu', (e) => {
        e.preventDefault();
        e.stopPropagation();
        const id = btn.getAttribute('data-proj-toggle');
        if (id === '__inbox') return;
        showProjectCtx(id, e.clientX, e.clientY);
      });
    });
    box.querySelectorAll('.proj-new').forEach((btn) => {
      btn.onclick = (e) => {
        e.preventDefault();
        e.stopPropagation();
        const id = btn.getAttribute('data-proj-new');
        newSessionInProject(id).catch((err) => alert(err.message || err));
      };
    });
    box.querySelectorAll('.proj-group[data-proj-id]').forEach((g) => {
      const id = g.getAttribute('data-proj-id');
      if (id && id !== '__inbox') {
        g.setAttribute('draggable', 'true');
        g.addEventListener('dragstart', (e) => {
          // don't start project drag from session row
          if (e.target.closest && e.target.closest('.thread,.proj-new')) {
            e.preventDefault();
            return;
          }
          g.classList.add('dragging');
          setDrag(e, { kind: 'project', id });
        });
        g.addEventListener('dragend', () => g.classList.remove('dragging'));
      }
      bindDropTarget(g, () => ({ type: 'project', projectId: id, unpinSession: true }));
      const kids = g.querySelector('.proj-children');
      if (kids) bindDropTarget(kids, () => ({ type: 'project', projectId: id, unpinSession: true }));
    });
  }
  function renderSessions() {
    const q = (state.filter || '').toLowerCase();
    const pins = loadPins();
    const pinSet = new Set(pins);
    const pinnedProjects = loadPinnedProjects();
    const pinProjSet = new Set(pinnedProjects);
    const filtered = state.sessions.filter(
      (s) => !q || (s.title || '').toLowerCase().includes(q) || String(s.id || '').includes(q)
    );
    const byId = new Map(filtered.map((s) => [String(s.id), s]));
    const map = loadSessionProjects();
    const projects = loadProjects();
    const expanded = loadProjectExpanded();

    // Sessions belonging to pinned projects
    const pinnedProjectSessionIds = new Set();
    for (const pid of pinnedProjects) {
      for (const s of filtered) {
        if (map[String(s.id)] === pid) pinnedProjectSessionIds.add(String(s.id));
      }
    }

    const pinnedSessions = [];
    for (const id of pins) {
      if (byId.has(id) && !pinnedProjectSessionIds.has(id)) pinnedSessions.push(byId.get(id));
    }

    // Unpinned = not in pin list and not under a pinned project group shown only in pin zone
    const unpinned = filtered.filter((s) => {
      const sid = String(s.id);
      if (pinSet.has(sid)) return false;
      const pj = map[sid];
      if (pj && pinProjSet.has(pj)) return false;
      return true;
    });

    const pinBox = $('pinnedList');
    const sessBox = $('sessionList');
    const projBox = $('projectList');
    const pinLabelEl = $('pinnedLabel');
    const sessLabel = $('sessionsLabel');
    if (pinLabelEl) pinLabelEl.textContent = 'Pinned';
    if (sessLabel) sessLabel.textContent = state.listMode === 'sessions' ? 'Sessions' : 'Projects';

    if (pinBox) {
      const blocks = [];
      for (const pid of pinnedProjects) {
        const proj = projects.find((p) => p.id === pid);
        if (!proj) continue;
        const items = filtered.filter((s) => map[String(s.id)] === pid);
        const isOpen = expanded[pid] !== false; // pinned projects default open
        const body = items.map((s) => threadRowHtml(s, true, pid)).join('');
        blocks.push(
          `<div class="proj-group${isOpen ? ' open' : ''}" data-proj-id="${esc(pid)}" data-pinned-project="1">` +
          `<button type="button" class="proj-head" data-proj-toggle="${esc(pid)}" title="${esc(proj.path || proj.name || '')}">` +
          `<span class="proj-chev">▸</span><span class="proj-ico">📁</span>` +
          `<span class="proj-name">${esc(proj.name || 'project')}</span>` +
          `<span class="proj-count">${items.length}</span>` +
          `<span class="proj-new" data-proj-new="${esc(pid)}" title="New session in project">+</span></button>` +
          `<div class="proj-children" data-proj-children="${esc(pid)}">${body || '<div class="cap-empty" style="padding:4px 6px;font-size:11px">No sessions</div>'}</div></div>`
        );
      }
      if (pinnedSessions.length) {
        blocks.push(pinnedSessions.map((s) => threadRowHtml(s, true)).join(''));
      }
      if (!blocks.length) {
        pinBox.innerHTML = '<div class="cap-empty" style="padding:4px 8px;font-size:11px">Drop sessions or projects here</div>';
      } else {
        pinBox.innerHTML = blocks.join('');
      }
      bindThreadList(pinBox);
      bindProjectChrome(pinBox);
      bindDropTarget(pinBox, () => ({ type: 'pin' }));
    }

    if (sessBox) {
      if (!unpinned.length) {
        sessBox.innerHTML = filtered.length
          ? '<div class="cap-empty" style="padding:6px 8px">All matching sessions are pinned</div>'
          : '<div class="cap-empty" style="padding:6px 8px">No sessions yet</div>';
      } else {
        sessBox.innerHTML = unpinned.map((s) => threadRowHtml(s, false)).join('');
        bindThreadList(sessBox);
      }
      sessBox.hidden = state.listMode !== 'sessions';
      bindDropTarget(sessBox, () => ({ type: 'session-list', unpinSession: true }));
    }

    if (projBox) {
      const used = new Set();
      const blocks = [];
      // Only non-pinned projects in main projects list
      const listProjects = projects.filter((p) => !pinProjSet.has(p.id));
      for (const proj of listProjects) {
        const items = unpinned.filter((s) => map[String(s.id)] === proj.id);
        items.forEach((s) => used.add(String(s.id)));
        const isOpen = expanded[proj.id] === true;
        const body = items.map((s) => threadRowHtml(s, false, proj.id)).join('');
        const pathTip = esc(proj.path || proj.name || '');
        blocks.push(
          `<div class="proj-group${isOpen ? ' open' : ''}" data-proj-id="${esc(proj.id)}">` +
          `<button type="button" class="proj-head" data-proj-toggle="${esc(proj.id)}" title="${pathTip}">` +
          `<span class="proj-chev">▸</span><span class="proj-ico">📁</span>` +
          `<span class="proj-name">${esc(proj.name || 'project')}</span>` +
          `<span class="proj-count">${items.length}</span>` +
          `<span class="proj-new" data-proj-new="${esc(proj.id)}" title="New session in project">+</span></button>` +
          `<div class="proj-children" data-proj-children="${esc(proj.id)}">${body || '<div class="cap-empty" style="padding:4px 6px;font-size:11px">No sessions</div>'}</div></div>`
        );
      }
      const inbox = unpinned.filter((s) => !used.has(String(s.id)));
      const inboxOpen = expanded['__inbox'] !== false;
      if (inbox.length || !listProjects.length) {
        const body = inbox.map((s) => threadRowHtml(s, false, '')).join('');
        blocks.push(
          `<div class="proj-group${inboxOpen ? ' open' : ''}" data-proj-id="__inbox">` +
          `<button type="button" class="proj-head" data-proj-toggle="__inbox" title="Unassigned sessions">` +
          `<span class="proj-chev">▸</span><span class="proj-ico">📥</span>` +
          `<span class="proj-name">Inbox</span>` +
          `<span class="proj-count">${inbox.length}</span>` +
          `<span class="proj-new" data-proj-new="__inbox" title="New session">+</span></button>` +
          `<div class="proj-children" data-proj-children="__inbox">${body || '<div class="cap-empty" style="padding:4px 6px;font-size:11px">No unassigned sessions</div>'}</div></div>`
        );
      }
      if (!blocks.length) {
        projBox.innerHTML = '<div class="cap-empty" style="padding:6px 8px">No projects yet — press + to add a folder</div>';
      } else {
        projBox.innerHTML = blocks.join('');
      }
      bindThreadList(projBox);
      bindProjectChrome(projBox);
      bindDropTarget(projBox, () => ({ type: 'project-list' }));
      projBox.hidden = state.listMode !== 'projects';
      if ($('projectAdd')) $('projectAdd').style.display = state.listMode === 'projects' ? '' : 'none';
    }
  }

  async function refreshSessions() {
    if (!hasNative()) return;
    try {
      const r = await api('sessions');
      const list = (r && r.sessions) || r || [];
      state.sessions = Array.isArray(list) ? list : [];
      const active = state.sessions.find((session) => String(session.id) === String(state.sessionId));
      if (active && active.created_at) setSessionClock(active.created_at);
    } catch (e) {
      console.warn('refreshSessions', e);
      state.sessions = state.sessions || [];
    }
    renderSessions();
  }
  async function openSession(id) {
    if (!id) return;
    const detail = await api('getSession', id);
    state.sessionId = detail.id;
    const meta = state.sessions.find((session) => String(session.id) === String(detail.id));
    setSessionClock(meta && meta.created_at);
    const raw = Array.isArray(detail.messages) ? detail.messages : [];
    const out = [];
    let pendingTools = [];
    for (const m of raw) {
      const role = m.role || 'assistant';
      const content = m.content || '';
      if (role === 'tool') continue;
      if (role === 'assistant' && typeof isToolCallJson === 'function' && isToolCallJson(content)) {
        try {
          const calls = JSON.parse(String(content).trim());
          if (Array.isArray(calls)) {
            pendingTools = pendingTools.concat(calls.map((c) => ({
              name: c.name || 'tool',
              detail: typeof c.arguments === 'string' ? c.arguments : JSON.stringify(c.arguments || {}),
              status: 'ok',
            })));
          }
        } catch {}
        continue;
      }
      if (role === 'assistant') {
        const text = typeof stripToolCallNoise === 'function' ? stripToolCallNoise(content) : content;
        out.push({
          role: 'assistant',
          content: text || content,
          tools: pendingTools.length ? pendingTools : undefined,
        });
        pendingTools = [];
      } else {
        out.push({ role: 'user', content });
      }
    }
    if (pendingTools.length) {
      out.push({ role: 'assistant', content: '', tools: pendingTools });
    }
    state.messages = out;
    setHeading(detail.title || 'session');
    renderSessions();
    renderMessages({ forceScroll: true });
  }
  async function newSession() {
    const s = await api('newSession');
    state.sessionId = s.id;
    setSessionClock();
    state.messages = [];
    setHeading(s.title || 'session');
    await refreshSessions();
    renderMessages();
    if ($('input')) $('input').focus();
    return s;
  }
  function timingPills(timings) {
    if (!timings) return '';
    const pill = (label, value) => value == null ? '' : `<span class="pill">${label} <em>${esc(formatDuration(value))}</em></span>`;
    return pill('total', timings.total_ms)
      + pill('first', timings.first_response_ms)
      + pill('model', timings.model_ms)
      + pill('tools', timings.tool_ms);
  }
  function setBusy(b) {
    state.busy = b;
    const button = $('send');
    button.classList.toggle('stop', b);
    button.disabled = b && state.cancelRequested;
    button.setAttribute('aria-label', b ? (state.cancelRequested ? 'Stopping' : 'Stop') : 'Send');
    button.title = b ? (state.cancelRequested ? 'Stopping…' : 'Stop generation') : 'Send (Enter)';
    button.innerHTML = b
      ? '<svg width="14" height="14" viewBox="0 0 24 24" aria-hidden="true"><rect x="7" y="7" width="10" height="10" fill="currentColor"/></svg>'
      : '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" aria-hidden="true"><path d="M12 19V5M5 12l7-7 7 7"/></svg>';
    // Keep input enabled so Enter feels responsive; only block double-send via busy flag
  }
  async function send() {
    const text = $('input').value.trim();
    if (state.busy) {
      state.cancelRequested = true;
      if (state.activeCancel) state.activeCancel();
      setBusy(true);
      return;
    }
    if (!text) return;
    state.cancelRequested = false;
    state.activeCancel = null;
    $('input').value = '';
    autoGrow();
    if (!hasNative()) {
      const start = state.messages.length;
      state.messages.push({ role: 'user', content: text });
      state.messages.push({
        role: 'assistant',
        content: 'Native bridge missing. Launch with: cargo run -p optimus-desktop',
        meta: `<span class="pill err"><em>no bridge</em></span>`,
      });
      renderMessages({ enterFrom: start, forceScroll: true });
      return;
    }
    try {
      if (!state.sessionId) await newSession();
    } catch (e) {
      const start = state.messages.length;
      state.messages.push({ role: 'user', content: text });
      state.messages.push({
        role: 'assistant',
        content: `Could not create session: ${e.message || e}`,
        meta: `<span class="pill err"><em>session</em></span>`,
      });
      renderMessages({ enterFrom: start, forceScroll: true });
      return;
    }
    const enterFrom = state.messages.length;
    state.messages.push({ role: 'user', content: text });
    const asstIdx = state.messages.length;
    state.messages.push({ role: 'assistant', content: '', streaming: true, tools: [] });
    state.tasks = [];
    renderTasks();
    renderMessages({ enterFrom, forceScroll: true });
    setBusy(true);
    startTurnClock();
    try {
      const provider = $('provider').value;
      const model = $('model').value;
      const thinkingOn = $('thinkingToggle').getAttribute('aria-pressed') === 'true';
      const fastOn = $('fastToggle').getAttribute('aria-pressed') === 'true';
      const thinkingLevel = thinkingOn ? $('thinkingLevel').value : 'off';
      const access = $('access').value;
      const opts = {
        provider,
        model,
        session: state.sessionId,
        thinking: thinkingOn,
        thinking_level: thinkingLevel,
        fast: fastOn,
        access,
        demo_memory: provider === 'offline' && /editor|prefer|memory/i.test(text),
      };
      const onEvent = (ev) => {
        if (!ev) return;
        if (ev.type === 'delta' && typeof ev.text === 'string') {
          // Don't paint raw tool-call JSON as prose
          if (isToolCallJson(ev.text) || /^\s*\[\s*\{\s*"id"\s*:\s*"call_/.test(ev.text)) {
            return;
          }
          let next = (state.messages[asstIdx].content || '') + ev.text;
          // If accumulation became tool JSON, drop it
          if (isToolCallJson(next)) next = '';
          else next = stripToolCallNoise(next) || next;
          // If we already have final-looking content and this looks like a new tool blob mid-stream, ignore
          state.messages[asstIdx].content = next;
          state.messages[asstIdx].streaming = true;
          scheduleStreamPaint(asstIdx);
        } else if (ev.type === 'status') {
          const typing = document.getElementById('typing');
          if (typing) typing.textContent = ev.text || 'Optimus is working…';
          else scheduleStreamPaint(asstIdx);
        } else if (ev.type === 'timing') {
          const timings = state.latestTurnTimings || (state.latestTurnTimings = {
            total_ms: null,
            first_response_ms: null,
            model_ms: 0,
            tool_ms: 0,
          });
          const duration = Number(ev.duration_ms) || 0;
          if (ev.kind === 'tool_started') {
            upsertTask(ev.name || 'tool', 'running', 'run', ev.call_id);
          }
          if (ev.kind === 'first_response') timings.first_response_ms = Number(ev.elapsed_ms) || 0;
          if (ev.kind === 'model_finished') timings.model_ms += duration;
          if (ev.kind === 'tool_finished' && !ev.suppressed) timings.tool_ms += duration;
          if (ev.kind === 'tool_finished') {
            const task = [...state.tasks].reverse().find((item) => item.name === ev.name);
            if (task) {
              task.durationMs = (Number(task.durationMs) || 0) + duration;
              if (ev.suppressed) task.suppressedCount = (task.suppressedCount || 0) + 1;
              task.status = ev.suppressed ? 'suppressed' : (ev.status === 'failed' ? 'failed' : 'ok');
              renderTasks();
            }
            const tools = state.messages[asstIdx].tools || [];
            const tool = [...tools].reverse().find((item) => item.name === ev.name);
            if (tool) tool.durationMs = (Number(tool.durationMs) || 0) + duration;
          }
          if (ev.kind === 'turn_finished') {
            timings.total_ms = Number(ev.duration_ms) || Number(ev.elapsed_ms) || 0;
            stopTurnClock(timings.total_ms);
          }
          scheduleStreamPaint(asstIdx);
        } else if (ev.type === 'tool') {
          const name = ev.name || 'tool';
          const detail = ev.detail || '';
          // Clear any leaked tool-call JSON when tools start
          if (isToolCallJson(state.messages[asstIdx].content)) {
            state.messages[asstIdx].content = '';
          } else {
            state.messages[asstIdx].content = stripToolCallNoise(state.messages[asstIdx].content);
          }
          const tools = state.messages[asstIdx].tools || (state.messages[asstIdx].tools = []);
          let t = tools.find(x => x.name === name && x.status === 'run');
          if (!t) {
            t = { name, detail, status: 'run' };
            tools.push(t);
          } else {
            t.detail = detail || t.detail;
            t.status = 'run';
          }
          const timedTask = state.tasks.find((item) => item.name === name && item.callIds && item.callIds.length);
          if (timedTask) {
            if (detail) timedTask.detail = detail;
            renderTasks();
          } else {
            upsertTask(name, detail, 'run');
          }
          scheduleStreamPaint(asstIdx);
        }
      };
      let res;
      if (window.optimus.chatStream) {
        const stream = window.optimus.chatStream(text, opts, onEvent);
        state.activeCancel = typeof stream.cancel === 'function' ? stream.cancel.bind(stream) : null;
        if (state.cancelRequested && state.activeCancel) state.activeCancel();
        res = await stream;
      } else {
        res = await api('chat', text, opts);
        state.messages[asstIdx].content = res.assistant_text || '';
      }
      state.sessionId = res.session_id || state.sessionId;
      if (res.title) setHeading(res.title);
      if (res.assistant_text) {
        state.messages[asstIdx].content = stripToolCallNoise(res.assistant_text);
      } else {
        state.messages[asstIdx].content = stripToolCallNoise(state.messages[asstIdx].content);
      }
      state.latestTurnTimings = res.timings || state.latestTurnTimings;
      stopTurnClock(state.latestTurnTimings && state.latestTurnTimings.total_ms);
      // Surface tool_trace from result if present
      if (Array.isArray(res.tool_trace) && res.tool_trace.length) {
        const durationAssigned = new Set();
        state.messages[asstIdx].tools = res.tool_trace.map(line => {
          const s = String(line);
          const name = s.split(/[\s:(]/)[0] || 'tool';
          const task = state.tasks.find((item) => item.name === name);
          const durationMs = task && !durationAssigned.has(name) ? task.durationMs : 0;
          durationAssigned.add(name);
          return { name, detail: s, status: 'ok', durationMs };
        });
      } else if (state.messages[asstIdx].tools) {
        state.messages[asstIdx].tools = state.messages[asstIdx].tools.map(t => ({ ...t, status: 'ok' }));
      }
      state.messages[asstIdx].streaming = false;
      state.messages[asstIdx].meta = `
        <span class="pill">provider <em>${esc(res.provider || provider)}</em></span>
        <span class="pill amber">model <em>${esc(model)}</em></span>
        <span class="pill">session <em>${esc(String(res.session_id || '').slice(0,8))}</em></span>
        <span class="pill">steps <em>${esc(res.steps)}</em></span>
        <span class="pill">schema <em>${esc(res.schema_tokens_final)}</em></span>
        ${thinkingOn ? `<span class="pill amber">think <em>${esc(thinkingLevel)}</em></span>` : ''}
        ${fastOn ? `<span class="pill">fast <em>on</em></span>` : ''}
        ${timingPills(state.latestTurnTimings)}`;
      finishTasks();
      paintStreamBubble(asstIdx);
      await refreshSessions();
    } catch (e) {
      const cancelled = state.cancelRequested || e?.name === 'AbortError' || /\bcancelled?\b/i.test(String(e?.message || e));
      if (cancelled) {
        const partial = state.messages[asstIdx] || { role: 'assistant', content: '' };
        partial.streaming = false;
        partial.content = stripToolCallNoise(partial.content || '') || 'Cancelled.';
        partial.meta = `<span class="pill amber"><em>cancelled</em></span>${timingPills(state.latestTurnTimings)}`;
        state.messages[asstIdx] = partial;
      } else {
        state.messages[asstIdx] = {
          role: 'assistant',
          content: `Error: ${e.message || e}`,
          meta: `<span class="pill err"><em>failed</em></span>${timingPills(state.latestTurnTimings)}`,
        };
      }
      finishTasks();
    } finally {
      if (state.turnStartedPerfMs != null) {
        stopTurnClock(state.latestTurnTimings && state.latestTurnTimings.total_ms);
      }
      state.activeCancel = null;
      state.cancelRequested = false;
      setBusy(false);
      renderMessages({ forceScroll: true });
      $('input').focus();
    }
  }
  function autoGrow() {
    const input = $('input');
    input.style.height = 'auto';
    input.style.height = Math.min(180, Math.max(52, input.scrollHeight)) + 'px';
  }
  function bindEnterToSend(el) {
    el.addEventListener('keydown', (e) => {
      if (e.key === 'Enter' && !e.shiftKey && !e.isComposing) {
        e.preventDefault();
        e.stopPropagation();
        send();
      }
    }, true);
  }
  function persistComposer() {
    try {
      localStorage.setItem('optimus.ui.composer', JSON.stringify({
        provider: $('provider').value,
        model: $('model').value,
        thinkingLevel: $('thinkingLevel').value,
        thinking: $('thinkingToggle').getAttribute('aria-pressed') === 'true',
        fast: $('fastToggle').getAttribute('aria-pressed') === 'true',
        access: $('access').value,
      }));
    } catch {}
  }
  function selectLabel(sel) {
    if (!sel) return '';
    const o = sel.options[sel.selectedIndex];
    return o ? o.textContent : sel.value;
  }
  function thinkButtonLabel() {
    const on = $('thinkingToggle').getAttribute('aria-pressed') === 'true';
    const fast = $('fastToggle').getAttribute('aria-pressed') === 'true';
    const lvl = $('thinkingLevel').value || 'off';
    if (!on || lvl === 'off') return fast ? 'off·fast' : 'off';
    return fast ? (lvl + '·fast') : lvl;
  }
  function syncComposerButtons() {
    if ($('provVal')) $('provVal').textContent = selectLabel($('provider'));
    if ($('modelVal')) $('modelVal').textContent = selectLabel($('model'));
    if ($('thinkVal')) $('thinkVal').textContent = thinkButtonLabel();
    if ($('accessVal')) $('accessVal').textContent = selectLabel($('access'));
    if ($('stModel')) {
      const em = $('stModel').querySelector('em');
      if (em) em.textContent = $('model').value;
    }
  }
  let _cddOpen = null; // data-cdd kind currently open
  function closeAllCdd() {
    _cddOpen = null;
    document.querySelectorAll('.cdd.open').forEach((el) => {
      el.classList.remove('open');
      const b = el.querySelector('.cdd-btn');
      if (b) b.setAttribute('aria-expanded', 'false');
    });
    const portal = $('cddPortal');
    if (portal) {
      portal.classList.remove('open');
      portal.innerHTML = '';
      portal.style.left = '';
      portal.style.top = '';
      portal.style.minWidth = '';
    }
  }
  function placePortal(anchorBtn) {
    const portal = $('cddPortal');
    if (!portal || !anchorBtn) return;
    portal.classList.add('open');
    const br = anchorBtn.getBoundingClientRect();
    const mw = Math.max(portal.scrollWidth || 160, br.width, 168);
    portal.style.minWidth = Math.round(mw) + 'px';
    const mh = portal.offsetHeight || 140;
    let left = br.left;
    let top = br.top - mh - 6; // always prefer ABOVE the chip
    if (top < 6) top = Math.min(br.bottom + 6, window.innerHeight - mh - 6);
    if (left + mw > window.innerWidth - 6) left = Math.max(6, window.innerWidth - mw - 6);
    if (left < 6) left = 6;
    portal.style.left = Math.round(left) + 'px';
    portal.style.top = Math.round(top) + 'px';
  }
  function buildCddHtml(kind) {
    if (kind === 'provider') {
      return Array.from($('provider').options).map((o) =>
        `<button type="button" role="option" class="${o.value === $('provider').value ? 'active' : ''}" data-kind="provider" data-v="${esc(o.value)}">${esc(o.textContent)}</button>`
      ).join('');
    }
    if (kind === 'model') {
      return Array.from($('model').options).map((o) =>
        `<button type="button" role="option" class="${o.value === $('model').value ? 'active' : ''}" data-kind="model" data-v="${esc(o.value)}">${esc(o.textContent)}</button>`
      ).join('');
    }
    if (kind === 'access') {
      return Array.from($('access').options).map((o) =>
        `<button type="button" role="option" class="${o.value === $('access').value ? 'active' : ''}" data-kind="access" data-v="${esc(o.value)}">${esc(o.textContent)}</button>`
      ).join('');
    }
    if (kind === 'think') {
      const lvl = $('thinkingLevel').value;
      const thinkOn = $('thinkingToggle').getAttribute('aria-pressed') === 'true';
      const fastOn = $('fastToggle').getAttribute('aria-pressed') === 'true';
      const levels = Array.from($('thinkingLevel').options).map((o) =>
        `<button type="button" role="menuitemradio" class="${o.value === lvl ? 'active' : ''}" data-kind="think-level" data-v="${esc(o.value)}" aria-checked="${o.value === lvl ? 'true' : 'false'}">${esc(o.textContent)}</button>`
      ).join('');
      return (
        `<div class="cdd-sec">Level</div>` + levels +
        `<div class="cdd-sep"></div>` +
        `<div class="cdd-sec">Modes</div>` +
        `<button type="button" class="cdd-tog" data-kind="think-on" aria-pressed="${thinkOn ? 'true' : 'false'}"><span>Thinking</span><span class="dot"></span></button>` +
        `<button type="button" class="cdd-tog" data-kind="think-fast" aria-pressed="${fastOn ? 'true' : 'false'}"><span>Fast</span><span class="dot"></span></button>`
      );
    }
    return '';
  }
  function bindPortalActions() {
    const portal = $('cddPortal');
    if (!portal) return;
    portal.querySelectorAll('button[data-kind]').forEach((btn) => {
      btn.addEventListener('click', (e) => {
        e.preventDefault();
        e.stopPropagation();
        const k = btn.getAttribute('data-kind');
        if (k === 'provider') {
          $('provider').value = btn.getAttribute('data-v');
          if ($('provider').value === 'offline') $('model').value = 'offline-echo';
          else if ($('model').value === 'offline-echo') $('model').value = 'gpt-5.6-terra';
          persistComposer(); syncComposerButtons(); closeAllCdd();
          return;
        }
        if (k === 'model') {
          $('model').value = btn.getAttribute('data-v');
          persistComposer(); syncComposerButtons(); closeAllCdd();
          return;
        }
        if (k === 'access') {
          $('access').value = btn.getAttribute('data-v');
          persistComposer(); syncComposerButtons(); closeAllCdd();
          return;
        }
        if (k === 'think-level') {
          const v = btn.getAttribute('data-v');
          $('thinkingLevel').value = v;
          $('thinkingToggle').setAttribute('aria-pressed', v === 'off' ? 'false' : 'true');
          persistComposer(); syncComposerButtons();
          openCddMenu('think', true);
          return;
        }
        if (k === 'think-on') {
          const on = btn.getAttribute('aria-pressed') === 'true';
          const next = !on;
          $('thinkingToggle').setAttribute('aria-pressed', next ? 'true' : 'false');
          if (next && $('thinkingLevel').value === 'off') $('thinkingLevel').value = 'medium';
          if (!next) $('thinkingLevel').value = 'off';
          persistComposer(); syncComposerButtons();
          openCddMenu('think', true);
          return;
        }
        if (k === 'think-fast') {
          const on = btn.getAttribute('aria-pressed') === 'true';
          $('fastToggle').setAttribute('aria-pressed', on ? 'false' : 'true');
          persistComposer(); syncComposerButtons();
          openCddMenu('think', true);
        }
      });
    });
  }
  function openCddMenu(kind, forceStay) {
    const cdd = document.querySelector('.cdd[data-cdd="' + kind + '"]');
    if (!cdd) return;
    if (!forceStay && _cddOpen === kind) {
      closeAllCdd();
      return;
    }
    document.querySelectorAll('.cdd.open').forEach((el) => {
      el.classList.remove('open');
      const b = el.querySelector('.cdd-btn');
      if (b) b.setAttribute('aria-expanded', 'false');
    });
    _cddOpen = kind;
    cdd.classList.add('open');
    const btn = cdd.querySelector('.cdd-btn');
    if (btn) btn.setAttribute('aria-expanded', 'true');
    const portal = $('cddPortal');
    if (!portal) return;
    portal.innerHTML = buildCddHtml(kind);
    portal.setAttribute('data-kind', kind);
    portal.setAttribute('role', kind === 'think' ? 'menu' : 'listbox');
    bindPortalActions();
    requestAnimationFrame(() => {
      placePortal(btn);
      requestAnimationFrame(() => placePortal(btn));
    });
  }
  function openCdd(cdd) {
    if (!cdd) return;
    const kind = cdd.getAttribute('data-cdd');
    if (!kind) return;
    openCddMenu(kind, false);
  }
  function bindComposerMenus() {
    document.querySelectorAll('.cdd').forEach((cdd) => {
      const btn = cdd.querySelector('.cdd-btn');
      if (!btn || btn.dataset.cddBound === '1') return;
      btn.dataset.cddBound = '1';
      btn.addEventListener('mousedown', (e) => {
        e.preventDefault();
        e.stopPropagation();
      });
      btn.addEventListener('click', (e) => {
        e.preventDefault();
        e.stopPropagation();
        openCdd(cdd);
      });
    });
    document.addEventListener('mousedown', (e) => {
      if (!_cddOpen) return;
      if (e.target.closest && (e.target.closest('#cddPortal') || e.target.closest('.cdd-btn'))) return;
      closeAllCdd();
    }, true);
    document.addEventListener('keydown', (e) => {
      if (e.key === 'Escape') closeAllCdd();
    });
    window.addEventListener('resize', () => {
      if (!_cddOpen) return;
      const cdd = document.querySelector('.cdd[data-cdd="' + _cddOpen + '"]');
      const btn = cdd && cdd.querySelector('.cdd-btn');
      if (btn) placePortal(btn);
    });
    window.addEventListener('scroll', () => {
      if (_cddOpen) closeAllCdd();
    }, true);
  }
  function restoreComposer() {
    try {
      const raw = localStorage.getItem('optimus.ui.composer');
      if (!raw) return;
      const c = JSON.parse(raw);
      if (c.provider) $('provider').value = c.provider;
      if (c.model) {
        const m = $('model');
        const ok = Array.from(m.options).some(o => o.value === c.model);
        m.value = ok ? c.model : (c.provider === 'offline' ? 'offline-echo' : 'gpt-5.6-terra');
      }
      if (c.thinkingLevel) {
        const t = $('thinkingLevel');
        const ok = Array.from(t.options).some(o => o.value === c.thinkingLevel);
        if (ok) t.value = c.thinkingLevel;
      }
      if (typeof c.thinking === 'boolean') $('thinkingToggle').setAttribute('aria-pressed', c.thinking ? 'true' : 'false');
      if (typeof c.fast === 'boolean') $('fastToggle').setAttribute('aria-pressed', c.fast ? 'true' : 'false');
      if (c.access) $('access').value = c.access;
    } catch {}
    if ($('provider').value === 'offline') $('model').value = 'offline-echo';
    // keep thinking toggle aligned with level
    if ($('thinkingLevel').value === 'off') $('thinkingToggle').setAttribute('aria-pressed', 'false');
    else if ($('thinkingToggle').getAttribute('aria-pressed') !== 'true') {
      /* level set but toggle off — honor level */
      $('thinkingToggle').setAttribute('aria-pressed', 'true');
    }
    syncComposerButtons();
  }
  bindComposerMenus();
  restoreComposer();
  syncComposerButtons();
  $('chat').addEventListener('scroll', () => {
    state.stickScroll = nearBottom($('chat'), 80);
  }, { passive: true });
  // Explicit wheel support (some WebView builds need overflow + this)
  $('chat').addEventListener('wheel', (e) => {
    // default scrolling works once overflow is fixed; keep stick logic honest
    requestAnimationFrame(() => { state.stickScroll = nearBottom($('chat'), 80); });
  }, { passive: true });
  $('taskChip').onclick = () => {
    const open = !$('taskPanel').classList.contains('open');
    setTaskPanel(open);
  };
  $('taskClose').onclick = () => setTaskPanel(false);
  $('send').onclick = () => { send(); };
  $('input').oninput = autoGrow;
  bindEnterToSend($('input'));
  $('newThread').onclick = () => {
    setRoute('chat');
    newSession().catch(err => {
      $('authBanner').className = 'auth-banner err';
      $('authBanner').textContent = err.message || String(err);
    });
  };
  if ($('navCapabilities')) $('navCapabilities').onclick = () => setRoute('capabilities');
  if ($('navMessaging')) $('navMessaging').onclick = () => setRoute('messaging');
  if ($('navArtifacts')) $('navArtifacts').onclick = () => setRoute('artifacts');
  loadLayout();
  applyLayout();
  setRoute(state.route || 'chat');

  // Right sidebar tabs
  state.rightTab = state.rightTab || 'files';
  function setRightTab(tab) {
    const t = ['files', 'artifacts', 'browser'].includes(tab) ? tab : 'files';
    state.rightTab = t;
    document.querySelectorAll('#rightPaneTabs button').forEach((b) => {
      b.classList.toggle('active', b.dataset.tab === t);
    });
    document.querySelectorAll('#rightPaneBody .rp-panel').forEach((p) => {
      p.classList.toggle('active', p.dataset.tab === t);
    });
    if (t === 'files' && state.layout.right) {
      try { loadFilesTree(state.filesPath || ''); } catch (e) {}
    }
  }
  document.querySelectorAll('#rightPaneTabs button').forEach((b) => {
    b.onclick = () => setRightTab(b.dataset.tab);
  });
  // Horizontal resize of right pane
  (function bindRightResize() {
    const handle = $('rightResize');
    if (!handle) return;
    let dragging = false;
    handle.addEventListener('pointerdown', (e) => {
      e.preventDefault();
      e.stopPropagation();
      dragging = true;
      handle.classList.add('dragging');
      const onMove = (ev) => {
        if (!dragging) return;
        const shell = document.querySelector('.shell') || document.body;
        const rect = shell.getBoundingClientRect();
        let w = rect.right - ev.clientX;
        w = Math.max(200, Math.min(Math.floor(rect.width * 0.55), w));
        document.documentElement.style.setProperty('--right-w', w + 'px');
      };
      const onUp = () => {
        dragging = false;
        handle.classList.remove('dragging');
        window.removeEventListener('pointermove', onMove);
        window.removeEventListener('pointerup', onUp);
        try { localStorage.setItem('optimus.ui.rightW', getComputedStyle(document.documentElement).getPropertyValue('--right-w').trim()); } catch {}
      };
      window.addEventListener('pointermove', onMove);
      window.addEventListener('pointerup', onUp);
    });
    try {
      const rw = localStorage.getItem('optimus.ui.rightW');
      if (rw) document.documentElement.style.setProperty('--right-w', rw);
    } catch {}
  })();
  if ($('filesRefresh')) {
    $('filesRefresh').onclick = () => loadFilesTree(state.filesPath || '');
  }

  if ($('toggleRight')) $('toggleRight').onclick = () => {
    state.layout.right = !state.layout.right;
    applyLayout();
    saveLayout();
    if (state.layout.right) loadFilesTree(state.filesPath || '');
  };
  if ($('toggleTerm')) $('toggleTerm').onclick = () => {
    state.layout.term = !state.layout.term;
    applyLayout();
    saveLayout();
  };
  if ($('toggleLogs')) $('toggleLogs').onclick = () => {
    state.layout.logs = !state.layout.logs;
    applyLayout();
    saveLayout();
  };
  if ($('logsClose')) $('logsClose').onclick = () => {
    state.layout.logs = false;
    applyLayout();
    saveLayout();
  };
  // Files pane: breadcrumb + tree + preview bound to fs_list / fs_read
  function normalizeFilesPath(p) {
    let s = String(p == null ? '' : p).replace(/\\/g, '/').trim();
    if (s === '.' || s === './') s = '';
    while (s.startsWith('./')) s = s.slice(2);
    s = s.replace(/\/+/g, '/').replace(/^\/+|\/+$/g, '');
    return s;
  }
  function parentFilesPath(p) {
    const s = normalizeFilesPath(p);
    if (!s) return '';
    const i = s.lastIndexOf('/');
    return i <= 0 ? '' : s.slice(0, i);
  }
  function formatSize(n) {
    const v = Number(n) || 0;
    if (v < 1024) return v + ' B';
    if (v < 1024 * 1024) return (v / 1024).toFixed(v < 10 * 1024 ? 1 : 0) + ' KB';
    return (v / (1024 * 1024)).toFixed(1) + ' MB';
  }
  async function fsListCall(path) {
    const p = normalizeFilesPath(path);
    if (window.optimus && typeof window.optimus.fsList === 'function') {
      return window.optimus.fsList(p);
    }
    return postRaw('fs_list', { path: p });
  }
  async function fsReadCall(path, maxBytes) {
    const p = normalizeFilesPath(path);
    if (window.optimus && typeof window.optimus.fsRead === 'function') {
      return window.optimus.fsRead(p, maxBytes || 512000);
    }
    return postRaw('fs_read', { path: p, max_bytes: maxBytes || 512000 });
  }
  function renderFilesCrumb(path) {
    const el = $('filesCrumb');
    const up = $('filesUp');
    if (!el) return;
    const p = normalizeFilesPath(path);
    const parts = p ? p.split('/').filter(Boolean) : [];
    const bits = [];
    bits.push(`<button type="button" class="crumb${parts.length ? '' : ' current'}" data-path="" title="Home">Home</button>`);
    let acc = '';
    parts.forEach((seg, idx) => {
      acc = acc ? acc + '/' + seg : seg;
      const isLast = idx === parts.length - 1;
      bits.push('<span class="crumb-sep">/</span>');
      if (isLast) {
        bits.push(`<span class="crumb current" title="${esc(acc)}">${esc(seg)}</span>`);
      } else {
        bits.push(`<button type="button" class="crumb" data-path="${esc(acc)}" title="${esc(acc)}">${esc(seg)}</button>`);
      }
    });
    el.innerHTML = bits.join('');
    el.querySelectorAll('button.crumb[data-path]').forEach((btn) => {
      btn.onclick = () => {
        const target = btn.getAttribute('data-path') || '';
        loadFilesTree(target);
      };
    });
    if (up) {
      up.disabled = !p;
      up.onclick = () => {
        if (!state.filesPath) return;
        loadFilesTree(parentFilesPath(state.filesPath));
      };
    }
  }
  function clearFilePreview() {
    state.filePreviewPath = '';
    const pre = $('filePreview');
    const pathEl = $('filePreviewPath');
    const badge = $('filePreviewBadge');
    if (pre) pre.textContent = '';
    if (pathEl) pathEl.textContent = '';
    if (badge) badge.hidden = true;
  }
  async function openFilePreview(path) {
    const pre = $('filePreview');
    const pathEl = $('filePreviewPath');
    const badge = $('filePreviewBadge');
    if (!pre) return;
    const p = normalizeFilesPath(path);
    state.filePreviewPath = p;
    if (pathEl) pathEl.textContent = p || '—';
    pre.textContent = 'Loading…';
    if (badge) badge.hidden = true;
    try {
      const r = await fsReadCall(p);
      if (state.filePreviewPath !== p) return;
      pre.textContent = (r && r.content != null) ? String(r.content) : '';
      if (badge) badge.hidden = !(r && r.truncated);
    } catch (e) {
      if (state.filePreviewPath !== p) return;
      pre.textContent = 'Error: ' + (e.message || String(e));
      if (badge) badge.hidden = true;
    }
  }
  function renderFilesEntries(entries) {
    const tree = $('filesTree');
    if (!tree) return;
    const list = Array.isArray(entries) ? entries.slice() : [];
    list.sort((a, b) => {
      const ak = (a.kind === 'dir' || a.kind === 'Dir') ? 0 : 1;
      const bk = (b.kind === 'dir' || b.kind === 'Dir') ? 0 : 1;
      if (ak !== bk) return ak - bk;
      return String(a.name || '').localeCompare(String(b.name || ''), undefined, { sensitivity: 'base' });
    });
    const rows = [];
    if (state.filesPath) {
      rows.push(`<div class="fs-row fs-up" data-kind="up" role="treeitem" title="Parent directory"><span class="fs-icon">↑</span><span class="fs-name">..</span></div>`);
    }
    list.forEach((ent) => {
      const kind = String(ent.kind || 'file').toLowerCase();
      const cls = kind === 'dir' ? 'fs-dir' : (kind === 'symlink' ? 'fs-symlink' : 'fs-file');
      const icon = kind === 'dir' ? '▸' : (kind === 'symlink' ? '↗' : '·');
      const meta = kind === 'file' && ent.size != null ? formatSize(ent.size) : '';
      rows.push(
        `<div class="fs-row ${cls}" data-kind="${esc(kind)}" data-path="${esc(ent.path || '')}" data-name="${esc(ent.name || '')}" role="treeitem" title="${esc(ent.path || ent.name || '')}">` +
        `<span class="fs-icon">${icon}</span><span class="fs-name">${esc(ent.name || ent.path || '?')}</span>` +
        (meta ? `<span class="fs-meta">${esc(meta)}</span>` : '') +
        `</div>`
      );
    });
    if (!rows.length) {
      tree.innerHTML = '<div class="fs-empty">Empty directory</div>';
      return;
    }
    tree.innerHTML = rows.join('');
    tree.querySelectorAll('.fs-row').forEach((row) => {
      row.onclick = () => {
        const kind = row.getAttribute('data-kind') || '';
        if (kind === 'up') {
          loadFilesTree(parentFilesPath(state.filesPath));
          return;
        }
        const path = row.getAttribute('data-path') || '';
        if (kind === 'dir') {
          loadFilesTree(path);
          return;
        }
        // files + symlinks: try read
        openFilePreview(path);
      };
    });
  }
  async function loadFilesTree(path) {
    const tree = $('filesTree');
    if (!tree) return;
    const p = normalizeFilesPath(path);
    state.filesPath = p;
    state.filesLoading = true;
    renderFilesCrumb(p);
    tree.innerHTML = '<div class="fs-empty">Loading…</div>';
    try {
      const r = await fsListCall(p);
      if (normalizeFilesPath(state.filesPath) !== p) return;
      const entries = (r && r.entries) || [];
      renderFilesEntries(entries);
    } catch (e) {
      if (normalizeFilesPath(state.filesPath) !== p) return;
      tree.innerHTML = `<div class="fs-error">${esc(e.message || e)}</div>`;
    } finally {
      state.filesLoading = false;
    }
  }
  // If layout restored with files open, load immediately
  if (state.layout.right) {
    loadFilesTree(state.filesPath || '');
  }
  window.loadFilesTree = loadFilesTree;
  $('refreshSessions').onclick = () => refreshSessions().catch(err => {
    $('authBanner').className = 'auth-banner err';
    $('authBanner').textContent = err.message || String(err);
  });
  $('sessionSearch').oninput = (e) => { state.filter = e.target.value; renderSessions(); };
  $('copySession').onclick = async () => {
    if (!state.sessionId) return;
    try { await navigator.clipboard.writeText(state.sessionId); } catch {}
  };
  $('importHermes').onclick = async () => {
    try {
      setBusy(true);
      $('authBanner').textContent = 'Importing Codex from Hermes…';
      const r = await api('authImportHermes');
      setAuthBanner(r.auth);
    } catch (e) {
      $('authBanner').className = 'auth-banner err';
      $('authBanner').textContent = e.message || String(e);
    } finally { setBusy(false); }
  };
  $('doctorBtn').onclick = async () => {
    try {
      const d = await api('doctor');
      updateStatusBar(d);
      alert(`Optimus ${d.version}\n${d.phase}\nhome: ${d.home}\ncodex: ${d.codex_present}\nbrowser: ${d.browser}\ncron: ${d.cron}\nschema: ${d.core_schema_tokens}\ncron_jobs: ${d.cron_jobs}\ncampaigns: ${d.campaigns_active}\napprovals: ${d.approvals_pending}`);
    } catch (e) { alert(e.message); }
  };
  async function refreshDoctorSnap() {
    const el = $('doctorSnap');
    const packs = $('packsSnap');
    if (!el) return;
    try {
      let d = null;
      if (hasNative() && window.optimus && typeof window.optimus.doctor === 'function') {
        d = await window.optimus.doctor();
      } else {
        d = await postRaw('doctor', {});
      }
      const tokens = Number(d.core_schema_tokens ?? 0);
      const maxB = Number(d.max_budget ?? 2500) || 2500;
      const pct = Math.max(0, Math.min(100, Math.round((tokens / maxB) * 100)));
      el.innerHTML = `
        <div>phase <em style="color:var(--text-2)">${esc(d.phase || '—')}</em> · ver <em style="color:var(--text-2)">${esc(d.version || '—')}</em></div>
        <div class="cap-budget">
          <div class="cap-budget-meta"><span>schema tokens</span><span>${tokens} / ${maxB} (${pct}%)</span></div>
          <div class="cap-budget-track"><div class="cap-budget-fill" id="capBudgetFill" style="width:${pct}%"></div></div>
        </div>
        <div style="margin-top:8px">cron_jobs <em style="color:var(--text-2)">${esc(d.cron_jobs)}</em> · campaigns_active <em style="color:var(--text-2)">${esc(d.campaigns_active)}</em> · approvals_pending <em style="color:var(--text-2)">${esc(d.approvals_pending)}</em></div>`;
      if (packs) {
        const catalog = Array.isArray(d.pack_catalog) ? d.pack_catalog : [];
        packs.innerHTML = catalog.length ? catalog.map((pack) => {
          const tools = Array.isArray(pack.tools) ? pack.tools : [];
          const toolText = tools.map((tool) => {
            const unavailable = tool.invocation === 'unavailable' ? ' (unavailable)' : '';
            return `${esc(tool.id)} · ${esc(tool.policy)}${unavailable}`;
          }).join(' · ');
          return `<div class="cap-row" data-pack-id="${esc(pack.id)}">
            <div class="cap-meta"><strong>${esc(pack.id)}</strong> · ${esc(pack.summary)}<br>${toolText || 'no tools'}</div>
            <div class="cap-actions"><span class="pill">${esc(pack.schema_tokens ?? tools.reduce((n, tool) => n + Number(tool.schema_tokens || 0), 0))} tok</span></div>
          </div>`;
        }).join('') : `<div class="cap-empty">No canonical pack catalog returned.</div>`;
      }
      updateCampaignStrip(d);
    } catch (e) {
      el.innerHTML = `<div class="cap-empty">${esc(e.message || e)}</div>`;
    }
  }
  function updateCampaignStrip(doc) {
    const strip = $('campaignStrip');
    if (!strip) return;
    const n = Number((doc && doc.campaigns_active) ?? 0);
    if (n > 0) {
      strip.classList.add('show');
      strip.innerHTML = `Campaigns active: <em>${n}</em>`;
    } else {
      strip.classList.remove('show');
      strip.textContent = '';
    }
  }
  async function refreshApprovals() {
    const el = $('approvalsList');
    if (!el) return;
    try {
      const r = await postRaw('approvals_list', {});
      const pending = (r && r.pending) || [];
      if (!pending.length) {
        el.innerHTML = '<div class="cap-empty">No pending approvals</div>';
        return;
      }
      el.innerHTML = pending.map((p) => {
        const label = p.job_label || p.node_label || p.job_id || 'job';
        const meta = `${label} · ${p.node_label || 'node'} · ${p.job_id || ''}`;
        return `<div class="cap-row" data-job-id="${esc(p.job_id)}">
          <div class="cap-meta" title="${esc(meta)}">${esc(meta)}</div>
          <div class="cap-actions">
            <button class="btn-mini ok" type="button" data-grant="${esc(p.job_id)}">Grant</button>
          </div>
        </div>`;
      }).join('');
      el.querySelectorAll('[data-grant]').forEach((btn) => {
        btn.onclick = async () => {
          const jobId = btn.getAttribute('data-grant');
          try {
            setBusy(true);
            await postRaw('approvals_grant', { job_id: jobId });
            await refreshApprovals();
            try { await refreshStatusBar(); } catch (e2) {}
          } catch (e) {
            if ($('authBanner')) {
              $('authBanner').className = 'auth-banner err';
              $('authBanner').textContent = e.message || String(e);
            }
          } finally { setBusy(false); }
        };
      });
    } catch (e) {
      el.innerHTML = `<div class="cap-empty">${esc(e.message || e)}</div>`;
    }
  }
  async function refreshCampaigns() {
    const el = $('campaignsList');
    if (!el) return;
    try {
      const r = await postRaw('campaign_list', {});
      const camps = (r && r.campaigns) || [];
      if (!camps.length) {
        el.innerHTML = '<div class="cap-empty">No campaigns</div>';
        return;
      }
      el.innerHTML = camps.slice(0, 24).map((c) => {
        const meta = `${c.name || c.id} · ${c.status || '—'}`;
        return `<div class="cap-row" data-camp-id="${esc(c.id)}">
          <div class="cap-meta" title="${esc(meta)}">${esc(meta)}</div>
          <div class="cap-actions">
            <button class="btn-mini" type="button" data-run="${esc(c.id)}">Run</button>
          </div>
        </div>`;
      }).join('');
      el.querySelectorAll('[data-run]').forEach((btn) => {
        btn.onclick = async () => {
          const id = btn.getAttribute('data-run');
          try {
            setBusy(true);
            const out = await postRaw('campaign_run', { id });
            const line = $('campaignStatusLine');
            if (line) line.textContent = `Run ${(id || '').slice(0, 8)}… → ${out && out.status ? out.status : 'ok'}`;
            await refreshCampaigns();
            try { await refreshStatusBar(); } catch (e2) {}
            try { await refreshDoctorSnap(); } catch (e3) {}
          } catch (e) {
            const line = $('campaignStatusLine');
            if (line) line.textContent = e.message || String(e);
          } finally { setBusy(false); }
        };
      });
    } catch (e) {
      el.innerHTML = `<div class="cap-empty">${esc(e.message || e)}</div>`;
    }
  }
  async function refreshCapabilitiesPage() {
    await Promise.all([
      refreshDoctorSnap().catch(function () {}),
      refreshApprovals().catch(function () {}),
      refreshCampaigns().catch(function () {}),
    ]);
  }
  if ($('capRefreshDoctor')) $('capRefreshDoctor').onclick = function () { refreshDoctorSnap(); };
  if ($('approvalsRefresh')) $('approvalsRefresh').onclick = function () { refreshApprovals(); };
  if ($('campaignsRefresh')) $('campaignsRefresh').onclick = function () { refreshCampaigns(); };
  if ($('campaignCreateBtn')) {
    $('campaignCreateBtn').onclick = async function () {
      try {
        setBusy(true);
        const name = 'ui-' + Date.now();
        await postRaw('campaign_create', {
          name: name,
          writes: [{ path: 'campaigns/ui-note.txt', contents: 'optimus campaign' }],
        });
        const line = $('campaignStatusLine');
        if (line) line.textContent = 'Created ' + name;
        await refreshCampaigns();
        try { await refreshStatusBar(); } catch (e2) {}
      } catch (e) {
        const line = $('campaignStatusLine');
        if (line) line.textContent = e.message || String(e);
      } finally { setBusy(false); }
    };
  }

  async function refreshCron() {
    const el = $('cronList');
    if (!el || !hasNative()) return;
    try {
      const r = await postRaw('cron_list', {});
      const jobs = (r && r.jobs) || [];
      if (!jobs.length) {
        el.textContent = 'No cron jobs';
        return;
      }
      // Hard cap display — never grow the sidebar past the locked cron block
      const MAX_SHOW = 6;
      const shown = jobs.slice(0, MAX_SHOW);
      const more = jobs.length - shown.length;
      el.innerHTML = shown.map(j =>
        `<div class="cron-row" title="${esc(j.name)}">• ${esc(j.name)} · ${esc(j.every_secs)}s · ${j.enabled ? 'on' : 'off'} · ${esc(j.last_status || '—')}</div>`
      ).join('') + (more > 0 ? `<div class="cron-row">+${more} more</div>` : '');
      if ($('stCron')) {
        const em = $('stCron').querySelector('em');
        if (em) em.textContent = String(jobs.length);
      }
    } catch (e) {
      el.textContent = (e.message || String(e)).slice(0, 120);
    }
  }
    async function postRaw(method, params) {
    const id = Math.floor(Math.random() * 1e9);
    // HTTP sandbox only
    if ((location.hostname === '127.0.0.1' || location.hostname === 'localhost') && location.port) {
      const r = await fetch('/api/ipc', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ id, method, params: params || {} }),
      });
      const msg = await r.json();
      if (!msg.ok) throw new Error(msg.error || 'ipc error');
      return msg.result;
    }
    if (!window.optimus) throw new Error('no bridge');
    const payload = JSON.stringify({ id, method, params: params || {} });
    return new Promise((resolve, reject) => {
      const prev = window.__optimusIpcReply;
      const timer = setTimeout(() => { window.__optimusIpcReply = prev; reject(new Error('timeout')); }, 30000);
      window.__optimusIpcReply = function (msg) {
        if (msg && msg.id === id) {
          clearTimeout(timer);
          window.__optimusIpcReply = prev;
          if (msg.ok) resolve(msg.result);
          else reject(new Error(msg.error || 'err'));
          return;
        }
        if (typeof prev === 'function') prev(msg);
      };
      if (window.ipc && window.ipc.postMessage) window.ipc.postMessage(payload);
      else if (window.chrome && window.chrome.webview && window.chrome.webview.postMessage) {
        window.chrome.webview.postMessage(payload);
      } else reject(new Error('no ipc'));
    });
  }
  if ($('cronTick')) {
    $('cronTick').onclick = async () => {
      try {
        setBusy(true);
        const r = await postRaw('cron_tick', {});
        $('authBanner').className = 'auth-banner ok';
        $('authBanner').textContent = `Cron tick: ${((r && r.ran) || []).length} ran`;
        await refreshCron();
        try { await refreshStatusBar(); } catch {}
      } catch (e) {
        $('authBanner').className = 'auth-banner err';
        $('authBanner').textContent = e.message || String(e);
      } finally { setBusy(false); }
    };
  }
  if ($('cronAdd')) {
    $('cronAdd').onclick = async () => {
      try {
        setBusy(true);
        // Cap UI-created jobs so spam-clicking cannot bloat store/UI
        try {
          const cur = await postRaw('cron_list', {});
          const n = ((cur && cur.jobs) || []).length;
          if (n >= 12) {
            $('authBanner').className = 'auth-banner err';
            $('authBanner').textContent = `Cron cap reached (${n}/12). Tick or remove jobs first.`;
            await refreshCron();
            return;
          }
        } catch {}
        const name = 'ui-' + Date.now().toString(36).slice(-4);
        await postRaw('cron_add', {
          name,
          every_secs: 300,
          prompt: 'Optimus desktop cron heartbeat',
          provider: 'offline',
        });
        await refreshCron();
        try { await refreshStatusBar(); } catch {}
        $('authBanner').className = 'auth-banner ok';
        $('authBanner').textContent = `Added cron ${name}`;
      } catch (e) {
        $('authBanner').className = 'auth-banner err';
        $('authBanner').textContent = e.message || String(e);
      } finally { setBusy(false); }
    };
  }
  function shortHome(p) {
    if (!p) return 'home…';
    const s = String(p).replace(/\\/g, '/');
    const parts = s.split('/').filter(Boolean);
    if (parts.length <= 2) return s.length > 28 ? ('…' + s.slice(-26)) : s;
    return '…/' + parts.slice(-2).join('/');
  }
  function updateStatusBar(d) {
    const doc = (d && d.doctor) || d || {};
    const setEm = (id, v) => {
      const el = $(id);
      if (!el) return;
      const em = el.querySelector('em');
      if (em) em.textContent = v;
      else el.textContent = v;
    };
    setEm('stGateway', doc.gateway === true ? 'ok' : (doc.gateway === false ? 'down' : (doc.gateway != null ? String(doc.gateway) : '—')));
    const agents = doc.campaigns_active ?? doc.agents ?? 0;
    const appr = Number(doc.approvals_pending || 0);
    setEm('stAgents', appr > 0 ? `${agents} · ${appr}appr` : String(agents));
    if ($('stAgents')) {
      $('stAgents').title = appr > 0 ? `${appr} approval(s) pending · ${agents} active campaign(s)` : `${agents} active campaign(s)`;
    }
    try { updateCampaignStrip(doc); } catch (e) {}
    setEm('stCron', doc.cron_jobs != null ? String(doc.cron_jobs) : '—');
    setEm('stTokens', doc.core_schema_tokens != null ? String(doc.core_schema_tokens) : '—');
    if ($('stModel') && $('model')) {
      const em = $('stModel').querySelector('em');
      if (em) em.textContent = $('model').value || '—';
    }
    if ($('stHome')) {
      const home = doc.home || ($('homeMeta') && $('homeMeta').textContent) || '';
      $('stHome').textContent = shortHome(home);
      $('stHome').title = home || '';
    }
    if ($('stVer')) $('stVer').textContent = doc.version || doc.phase || 'desktop';
  }
  async function refreshStatusBar() {
    try {
      let doc = null;
      if (hasNative() && window.optimus && typeof window.optimus.doctor === 'function') {
        doc = await window.optimus.doctor();
      } else if ((location.hostname === '127.0.0.1' || location.hostname === 'localhost') && location.port) {
        doc = await postRaw('doctor', {});
      }
      if (doc) updateStatusBar(doc);
      else updateStatusBar({});
    } catch {
      updateStatusBar({});
    }
  }
  let _statusPollTimer = null;

  async function termRunLine(line) {
    const out = $('termOut');
    if (!out) return;
    line = String(line || '').trim();
    if (!line) return;
    out.textContent += (out.textContent.endsWith('\n') || !out.textContent ? '' : '\n') + '$ ' + line + '\n';
    out.scrollTop = out.scrollHeight;
    try {
      let r;
      if (window.optimus && typeof window.optimus.termRun === 'function') r = await window.optimus.termRun(line);
      else r = await postRaw('term_run', { line });
      const so = (r && r.stdout) || '';
      const se = (r && r.stderr) || '';
      if (so) out.textContent += so.replace(/\r\n/g, '\n');
      if (se) out.textContent += (so && !so.endsWith('\n') ? '\n' : '') + se.replace(/\r\n/g, '\n');
      if (r && r.exit_code != null) out.textContent += (out.textContent.endsWith('\n') ? '' : '\n') + '[exit ' + r.exit_code + (r.timed_out ? ' timeout' : '') + ']\n';
    } catch (e) {
      out.textContent += 'error: ' + (e.message || e) + '\n';
    }
    out.scrollTop = out.scrollHeight;
  }
  if ($('termIn')) {
    $('termIn').addEventListener('keydown', (e) => {
      if (e.key === 'Enter') {
        e.preventDefault();
        const v = $('termIn').value;
        $('termIn').value = '';
        termRunLine(v);
      }
    });
  }

  function startStatusPoll() {
    if (_statusPollTimer) return;
    _statusPollTimer = setInterval(() => { refreshStatusBar(); }, 15000);
  }
  async function applyReady(d) {
    if (!d) return;
    if (state.booted) {
      // refresh auth/home only
      if (d.doctor) {
        $('homeMeta').textContent = d.doctor.home || '';
        $('branchMeta').textContent = `${d.doctor.phase || 'desktop'} · schema ${d.doctor.core_schema_tokens ?? ''}`;
        updateStatusBar(d);
      }
      if (d.auth) setAuthBanner(d.auth);
      startStatusPoll();
      return;
    }
    state.booted = true;
    try { setListMode(state.listMode || 'projects'); } catch (e) {}

    if (d.doctor) {
      $('homeMeta').textContent = d.doctor.home || '';
      $('branchMeta').textContent = `${d.doctor.phase || 'desktop'} · schema ${d.doctor.core_schema_tokens ?? ''}`;
      updateStatusBar(d);
    }
    if (d.auth) setAuthBanner(d.auth);
    else {
      try { setAuthBanner(await api('authStatus')); } catch {}
    }
    // Prefer offline when Codex is missing so first-run chat always works
    try {
      const auth = d.auth || {};
      if (!auth.present) {
        const provider = $('provider');
        if (provider) {
          provider.value = 'offline';
          const model = $('model');
          if (model) model.value = 'offline-echo';
        }
      }
    } catch {}
    if (d.sessions) {
      state.sessions = Array.isArray(d.sessions) ? d.sessions : [];
      renderSessions();
    } else {
      try { await refreshSessions(); } catch {}
    }
    if (!state.sessionId) {
      if (state.sessions[0]) {
        try { await openSession(state.sessions[0].id); } catch { await newSession(); }
      } else {
        try { await newSession(); } catch (e) {
          $('authBanner').className = 'auth-banner err';
          $('authBanner').textContent = e.message || String(e);
        }
      }
    }
    try { await refreshCron(); } catch {}
    try { await refreshStatusBar(); } catch {}
    startStatusPoll();
    document.documentElement.dataset.bootState = 'ready';
  }

  if ($('model')) {
    $('model').addEventListener('change', () => updateStatusBar({}));
  }

  window.addEventListener('optimus-ready', (ev) => {
    applyReady(ev.detail || {}).catch((e) => {
      document.documentElement.dataset.bootState = 'error';
      $('authBanner').className = 'auth-banner err';
      $('authBanner').textContent = e.message || String(e);
    });
  });
  // Self-bootstrap: don't depend solely on native push timing
  let bootPromise = null;
  async function bootstrap() {
    if (state.booted) return;
    if (bootPromise) return bootPromise;
    bootPromise = (async () => {
      $('authBanner').textContent = 'Connecting to Kernel…';
      let lastErr = null;
      for (let i = 0; i < 50; i++) {
        if (state.booted) return;
        if (window.__optimusReadyDetail) {
          await applyReady(window.__optimusReadyDetail);
          return;
        }
        if (typeof window.optimus !== 'undefined' && hasNative()) {
          try {
            const auth = await window.optimus.authStatus();
            const doctor = await window.optimus.doctor();
            const sessionsWrap = await window.optimus.sessions();
            await applyReady({
              auth,
              doctor,
              sessions: sessionsWrap.sessions || sessionsWrap || [],
            });
            return;
          } catch (e) {
            lastErr = e;
          }
        }
        await new Promise((r) => setTimeout(r, 50));
      }
      if (!state.booted) {
        document.documentElement.dataset.bootState = 'error';
        $('authBanner').className = 'auth-banner err';
        $('authBanner').textContent = lastErr
          ? (`Bridge timeout: ${lastErr.message || lastErr}`)
          : 'Native bridge missing — run optimus-desktop.exe';
        renderMessages();
      }
    })();
    return bootPromise;
  }
  // Start once
  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', () => { bootstrap(); });
  } else {
    bootstrap();
  }
})();
