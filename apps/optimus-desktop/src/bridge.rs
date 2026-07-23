//! Bridge JS shared by WebView init + HTML inject.

pub const BRIDGE_JS: &str = r#"
(function () {
  if (window.__optimusBridgeInstalled) return;
  window.__optimusBridgeInstalled = true;

  const pending = new Map();
  const streamHandlers = new Map();
  let seq = 1;

  function isHttpMode() {
    // ONLY the Playwright/dev HTTP server (127.0.0.1:PORT) uses fetch IPC.
    // Native WebView serves UI through Wry's custom protocol. WebView2/Android
    // expose http://optimus.localhost/; WebKitGTK/WebKit use optimus://localhost/.
    // Both MUST use window.ipc, not fetch (fetch hits the asset handler and dies).
    try {
      if (window.__OPTIMUS_HTTP_MODE__ === true) return true;
      var host = location.hostname || '';
      var port = location.port || '';
      if ((host === '127.0.0.1' || host === 'localhost') && port !== '') return true;
      return false;
    } catch (e) { return false; }
  }

  const nativeFetch = window.fetch ? window.fetch.bind(window) : null;
  if (nativeFetch && isHttpMode()) {
    window.fetch = function (input, init) {
      const target = typeof input === 'string' ? input : String(input && input.url || '');
      if (target.startsWith('/api/') || target.startsWith(location.origin + '/api/')) {
        const next = Object.assign({}, init || {});
        next.headers = Object.assign({}, next.headers || {}, {
          'Authorization': 'Bearer ' + String(window.__OPTIMUS_HTTP_TOKEN__ || ''),
          'X-Optimus-CSRF': '1',
        });
        return nativeFetch(input, next);
      }
      return nativeFetch(input, init);
    };
  }

  function postNative(payload) {
    if (window.ipc && typeof window.ipc.postMessage === 'function') {
      window.ipc.postMessage(payload);
      return true;
    }
    if (window.chrome && window.chrome.webview && typeof window.chrome.webview.postMessage === 'function') {
      window.chrome.webview.postMessage(payload);
      return true;
    }
    return false;
  }

  function httpHeaders() {
    return {
      'Content-Type': 'application/json',
      'Authorization': 'Bearer ' + String(window.__OPTIMUS_HTTP_TOKEN__ || ''),
      'X-Optimus-CSRF': '1',
    };
  }

  function post(method, params) {
    const id = seq++;
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        if (pending.has(id)) {
          pending.delete(id);
          reject(new Error('IPC timeout: ' + method));
        }
      }, 180000);
      pending.set(id, {
        resolve: (v) => { clearTimeout(timer); resolve(v); },
        reject: (e) => { clearTimeout(timer); reject(e); },
      });
      const envelope = { id: id, method: method, params: params || {} };
      const payload = JSON.stringify(envelope);
      try {
        if (isHttpMode()) {
          fetch('/api/ipc', {
            method: 'POST',
            headers: httpHeaders(),
            body: payload,
          }).then(async (r) => {
            const msg = await r.json();
            window.__optimusIpcReply(msg);
          }).catch((e) => {
            pending.delete(id);
            clearTimeout(timer);
            reject(e);
          });
          return;
        }
        if (!postNative(payload)) {
          pending.delete(id);
          clearTimeout(timer);
          reject(new Error('Native bridge missing — run optimus-desktop'));
        }
      } catch (e) {
        pending.delete(id);
        clearTimeout(timer);
        reject(e);
      }
    });
  }

  window.__optimusIpcReply = function (msg) {
    try {
      if (msg && msg.result && msg.result.event === 'ready') {
        window.__optimusReadyDetail = msg.result;
        window.dispatchEvent(new CustomEvent('optimus-ready', { detail: msg.result }));
      }
      const p = pending.get(msg.id);
      if (!p) return;
      pending.delete(msg.id);
      if (msg.ok) p.resolve(msg.result);
      else p.reject(new Error(msg.error || 'ipc error'));
    } catch (e) {
      console.error('optimus ipc reply error', e);
    }
  };

  // Native WebView multi-event stream push
  window.__optimusStream = function (id, ev) {
    const h = streamHandlers.get(id);
    if (!h) return;
    try {
      if (ev && ev.type === 'done') {
        streamHandlers.delete(id);
        h.resolve(ev.result || {});
      } else if (ev && ev.type === 'error') {
        streamHandlers.delete(id);
        h.reject(new Error(ev.error || 'stream error'));
      } else if (h.onEvent) {
        h.onEvent(ev);
      }
    } catch (e) {
      console.error(e);
    }
  };

  function attachCancel(task, action) {
    let cancelOpen = true;
    task.then(
      function () { cancelOpen = false; },
      function () { cancelOpen = false; }
    );
    const cancelOnce = function () {
      if (!cancelOpen) return false;
      cancelOpen = false;
      action();
      return true;
    };
    task.cancel = cancelOnce;
    return task;
  }

  function chatStreamHttp(message, opts, onEvent) {
    const controller = new AbortController();
    const task = (async function () {
      const body = Object.assign({ message: message }, opts || {});
      const r = await fetch('/api/chat/stream', {
        method: 'POST',
        headers: httpHeaders(),
        body: JSON.stringify(body),
        signal: controller.signal,
      });
      if (!r.ok) {
        const t = await r.text();
        throw new Error('stream HTTP ' + r.status + ': ' + t);
      }
      const reader = r.body.getReader();
      const dec = new TextDecoder();
      let buf = '';
      let finalResult = null;
      while (true) {
        const { done, value } = await reader.read();
        if (done) break;
        buf += dec.decode(value, { stream: true });
        let idx;
        while ((idx = buf.indexOf('\n\n')) >= 0) {
          const block = buf.slice(0, idx);
          buf = buf.slice(idx + 2);
          const lines = block.split('\n');
          for (const line of lines) {
            if (!line.startsWith('data: ')) continue;
            const data = line.slice(6).trim();
            if (!data || data.startsWith(':')) continue;
            let ev;
            try { ev = JSON.parse(data); } catch (e) { continue; }
            if (ev.type === 'done') {
              finalResult = ev.result || {};
            } else if (ev.type === 'error') {
              throw new Error(ev.error || 'stream error');
            } else if (onEvent) {
              onEvent(ev);
            }
          }
        }
      }
      if (!finalResult) throw new Error('stream ended without done');
      return finalResult;
    })();
    return attachCancel(task, function () { controller.abort(); });
  }

  function chatStreamNative(message, opts, onEvent) {
    const id = seq++;
    const task = new Promise((resolve, reject) => {
      streamHandlers.set(id, { resolve, reject, onEvent });
      const payload = JSON.stringify({
        id: id,
        method: 'chat_stream',
        params: Object.assign({ message: message }, opts || {}),
      });
      if (!postNative(payload)) {
        streamHandlers.delete(id);
        reject(new Error('Native bridge missing'));
      }
    });
    return attachCancel(task, function () {
      post('chat_cancel', { stream_id: id }).catch(function (error) {
        const handler = streamHandlers.get(id);
        if (!handler) return;
        streamHandlers.delete(id);
        handler.reject(error);
      });
    });
  }

  window.optimus = {
    doctor: function () { return post('doctor'); },
    settingsGet: function () { return post('settings_get'); },
    settingsSet: function (params) { return post('settings_set', params || {}); },
    sessions: function () { return post('sessions'); },
    getSession: function (id) { return post('get_session', { id: id }); },
    newSession: function () { return post('new_session'); },
    chat: function (message, opts) {
      var o = opts || {};
      return post('chat', Object.assign({ message: message }, o));
    },
    chatStream: function (message, opts, onEvent) {
      if (isHttpMode()) return chatStreamHttp(message, opts, onEvent);
      return chatStreamNative(message, opts, onEvent);
    },
    chatOffline: function (message, opts) {
      var o = opts || {};
      o.provider = 'offline';
      return post('chat', Object.assign({ message: message }, o));
    },
    authStatus: function () { return post('auth_status'); },
    authImportHermes: function () { return post('auth_import_hermes'); },
    authImportCli: function () { return post('auth_import_cli'); },
    ping: function () { return post('ping'); },
    approvalsList: function () { return post('approvals_list'); },
    approvalsGrant: function (jobId) { return post('approvals_grant', { job_id: jobId }); },
    campaignList: function () { return post('campaign_list'); },
    campaignCreate: function (name, writes) {
      return post('campaign_create', { name: name, writes: writes || [] });
    },
    campaignRun: function (id) { return post('campaign_run', { id: id }); },
    campaignStatus: function (id) { return post('campaign_status', { id: id }); },
    windowMinimize: function () { return post('window_minimize'); },
    windowMaximize: function () { return post('window_maximize'); },
    windowClose: function () { return post('window_close'); },
    windowDrag: function () { return post('window_drag'); },
    windowOuterPosition: function () { return post('window_outer_position'); },
    windowSetOuterPosition: function (x, y) {
      return post('window_set_outer_position', { x: x|0, y: y|0 });
    },
    fsRoots: function () { return post('fs_roots'); },
    fsList: function (path) { return post('fs_list', { path: path || '' }); },
    fsRead: function (path, maxBytes) {
      return post('fs_read', { path: path, max_bytes: maxBytes || 512000 });
    },
    termRun: function (line) { return post('term_run', { line: line || '' }); },
    pickFolder: function () { return post('pick_folder'); },
    deleteSession: function (id) { return post('delete_session', { id: id }); },
    renameSession: function (id, title) { return post('rename_session', { id: id, title: title }); },
    openPath: function (path) { return post('open_path', { path: path }); },
    openUrl: function (url) { return post('open_url', { url: url }); },
    invoke: function (method, params) { return post(method, params || {}); },
  };

  window.__optimusHasNative = function () {
    if (isHttpMode()) return true;
    return !!(window.ipc || (window.chrome && window.chrome.webview));
  };
})();
"#;

pub fn inject_bridge(html: &str) -> String {
    let head_tag = format!("<script>\n{BRIDGE_JS}\n</script>\n");
    let mut out = html.to_string();
    if let Some(idx) = out.find("</head>") {
        out.insert_str(idx, &head_tag);
    }
    if let Some(idx) = out.rfind("</body>") {
        out.insert_str(idx, &head_tag);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::BRIDGE_JS;

    #[test]
    fn stream_promises_expose_local_idempotent_cancellation() {
        assert!(BRIDGE_JS.contains("const controller = new AbortController();"));
        assert!(BRIDGE_JS.contains("signal: controller.signal"));
        assert!(BRIDGE_JS.contains("task.cancel = cancelOnce"));
        assert!(BRIDGE_JS.contains("post('chat_cancel', { stream_id: id })"));
    }

    #[test]
    fn external_links_use_the_validated_os_ipc() {
        assert!(BRIDGE_JS.contains("openUrl: function (url)"));
        assert!(BRIDGE_JS.contains("post('open_url', { url: url })"));
    }
}
