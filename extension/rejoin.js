(() => {
  window.addEventListener("voice-watch-last-server", (event) => {
    const server = normalizeServer(event.detail);
    if (!server) {
      return;
    }

    chrome.runtime
      .sendMessage({ type: "remember_last_server", server })
      .catch(() => {});
  });

  injectPageScript({
    rejoin: rejoinPayloadFromUrl(),
    pagePlaceId: placeIdFromPath()
  });

  function rejoinPayloadFromUrl() {
    const params = new URLSearchParams(window.location.search);
    const gameInstanceId =
      cleanGameInstanceId(params.get("gameInstanceId")) ||
      cleanGameInstanceId(params.get("serverJobId"));
    const payload = {
      placeId: numberString(params.get("placeId")) || placeIdFromPath(),
      gameInstanceId,
      accessCode: clean(params.get("accessCode")),
      linkCode: clean(params.get("linkCode"))
    };

    if (params.get("voiceWatchRejoin") !== "1" && !params.get("serverJobId")) {
      return null;
    }

    cleanupUrl(params);
    return payload.placeId ? payload : null;
  }

  function injectPageScript(payload) {
    const target = document.documentElement || document.head || document.body;
    if (!target) {
      window.addEventListener("DOMContentLoaded", () => injectPageScript(payload), {
        once: true
      });
      return;
    }

    const script = document.createElement("script");
    script.src = chrome.runtime.getURL("rejoin_page.js");
    script.dataset.voiceWatchPayload = JSON.stringify(payload);
    script.onload = () => script.remove();
    target.append(script);
  }

  function cleanupUrl(urlParams) {
    for (const key of [
      "voiceWatchRejoin",
      "placeId",
      "gameInstanceId",
      "serverJobId",
      "accessCode",
      "linkCode"
    ]) {
      urlParams.delete(key);
    }

    const cleanSearch = urlParams.toString();
    const nextUrl =
      window.location.pathname +
      (cleanSearch ? `?${cleanSearch}` : "") +
      window.location.hash;
    window.history.replaceState(null, "", nextUrl);
  }

  function placeIdFromPath() {
    const match = window.location.pathname.match(/\/games\/(\d+)/i);
    return match ? match[1] : null;
  }

  function numberString(value) {
    const cleaned = clean(value);
    return cleaned && /^\d+$/.test(cleaned) ? cleaned : null;
  }

  function cleanGameInstanceId(value) {
    const cleaned = clean(value);
    return cleaned && /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(cleaned)
      ? cleaned
      : null;
  }

  function clean(value) {
    const cleaned = String(value || "").trim();
    return cleaned.length > 0 ? cleaned : null;
  }

  function normalizeServer(server) {
    const placeId = numberString(server?.placeId);
    const gameInstanceId = cleanGameInstanceId(server?.gameInstanceId);
    const accessCode = clean(server?.accessCode);
    const linkCode = clean(server?.linkCode);
    if (!placeId || (!gameInstanceId && !accessCode && !linkCode)) {
      return null;
    }

    return {
      placeId: Number(placeId),
      gameInstanceId,
      accessCode,
      linkCode,
      detectedAtMs: Date.now()
    };
  }
})();
