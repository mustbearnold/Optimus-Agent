// Trunk-only adapter. The real Wry shell already injects window.optimus.
(function () {
  if (window.optimus && typeof window.optimus.invoke === 'function') return;
  let seq = 1;
  window.optimus = {
    async invoke(method, params) {
      const response = await fetch('/api/ipc', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ id: seq++, method, params: params || {} }),
      });
      const reply = await response.json();
      if (!response.ok || !reply.ok) {
        throw new Error(reply.error || `IPC ${response.status}`);
      }
      return reply.result;
    },
  };
})();
