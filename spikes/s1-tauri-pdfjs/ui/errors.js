// Classic script, loaded before the module: report any page error through the bench channel.
(function () {
  const send = (what) => {
    try {
      document.getElementById("status").textContent = "ERROR: " + what;
      window.__TAURI__?.core?.invoke("report", { result: { error: what } });
    } catch (_) { /* nothing else to do in a spike */ }
  };
  window.addEventListener("error", (e) => send(`${e.message} @ ${e.filename}:${e.lineno}`));
  window.addEventListener("unhandledrejection", (e) => send(String(e.reason?.stack ?? e.reason)));
  window.addEventListener("securitypolicyviolation", (e) => send(`CSP blocked ${e.blockedURI} (${e.violatedDirective})`));
  setTimeout(() => { if (!window.__spikeStarted) send("module did not start within 10 s"); }, 10000);
})();
