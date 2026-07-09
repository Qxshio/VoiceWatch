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

  injectPageScript(captureAndLaunchFromPageContext, {
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

  function injectPageScript(fn, payload) {
    const target = document.documentElement || document.head || document.body;
    if (!target) {
      window.addEventListener("DOMContentLoaded", () => injectPageScript(fn, payload), {
        once: true
      });
      return;
    }

    const script = document.createElement("script");
    script.textContent = `(${fn.toString()})(${JSON.stringify(payload)});`;
    target.append(script);
    script.remove();
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

  function captureAndLaunchFromPageContext(payload) {
    const startedAt = Date.now();
    const timeoutMs = 30000;
    const pagePlaceId = cleanNumberString(payload.pagePlaceId);
    const rejoin = normalize(payload.rejoin);

    function tryInstall() {
      const launcher = window.Roblox && window.Roblox.GameLauncher;
      if (!launcher) {
        retryOrFallback();
        return;
      }

      installCaptureHooks(launcher);
      if (rejoin) {
        launch(rejoin, true);
      }
    }

    function installCaptureHooks(launcher) {
      if (launcher.__voiceWatchWrapped) {
        return;
      }

      wrap(launcher, "joinGameInstance", (args) => ({
        placeId: args[0],
        gameInstanceId: args[1]
      }));
      wrap(launcher, "joinPrivateGame", (args) => ({
        placeId: args[0],
        accessCode: args[1]
      }));
      wrap(launcher, "joinGame", (args) => ({
        placeId: args[0],
        gameInstanceId: args[1]
      }));
      wrap(launcher, "openGame", (args) => ({
        placeId: args[0],
        gameInstanceId: args[1]
      }));

      launcher.__voiceWatchWrapped = true;
    }

    function wrap(object, name, toServer) {
      const original = object[name];
      if (typeof original !== "function" || original.__voiceWatchWrapped) {
        return;
      }

      function wrapped(...args) {
        remember(toServer(args));
        return original.apply(this, args);
      }
      wrapped.__voiceWatchWrapped = true;
      object[name] = wrapped;
    }

    function launch(server, allowRetry) {
      const launcher = window.Roblox && window.Roblox.GameLauncher;
      const placeId = Number(server.placeId);
      if (!launcher || !Number.isFinite(placeId)) {
        if (allowRetry) {
          retryOrFallback();
        }
        return;
      }

      try {
        remember(server);
        window._skipRobloxJoinInterceptor = true;
        if (server.gameInstanceId && typeof launcher.joinGameInstance === "function") {
          launcher.joinGameInstance(placeId, server.gameInstanceId);
          return;
        }

        const privateCode = server.accessCode || server.linkCode;
        if (privateCode && typeof launcher.joinPrivateGame === "function") {
          launcher.joinPrivateGame(placeId, privateCode);
          return;
        }
      } catch (error) {
        console.warn("[Voice Watch] Roblox launcher call failed, falling back.", error);
      }

      fallbackToGameStart(server);
    }

    function retryOrFallback() {
      if (Date.now() - startedAt < timeoutMs) {
        window.setTimeout(tryInstall, 250);
        return;
      }

      if (!rejoin) {
        window.setTimeout(tryInstall, 1000);
        return;
      }

      if (rejoin) {
        fallbackToGameStart(rejoin);
      }
    }

    function fallbackToGameStart(server) {
      const query = new URLSearchParams({ placeId: String(server.placeId) });
      if (server.gameInstanceId) {
        query.set("gameInstanceId", server.gameInstanceId);
      }
      if (server.accessCode) {
        query.set("accessCode", server.accessCode);
      }
      if (server.linkCode) {
        query.set("linkCode", server.linkCode);
      }

      window.location.assign(`https://www.roblox.com/games/start?${query.toString()}`);
    }

    function remember(server) {
      const normalized = normalize(server);
      if (!normalized) {
        return;
      }

      window.dispatchEvent(
        new CustomEvent("voice-watch-last-server", {
          detail: normalized
        })
      );
    }

    function normalize(server) {
      const placeId = cleanNumberString(server && server.placeId) || pagePlaceId;
      const gameInstanceId = cleanGameInstanceId(server && server.gameInstanceId);
      const accessCode = cleanString(server && server.accessCode);
      const linkCode = cleanString(server && server.linkCode);
      if (!placeId || (!gameInstanceId && !accessCode && !linkCode)) {
        return null;
      }

      return {
        placeId,
        gameInstanceId,
        accessCode,
        linkCode
      };
    }

    function cleanNumberString(value) {
      const cleaned = cleanString(value);
      return cleaned && /^\d+$/.test(cleaned) ? cleaned : null;
    }

    function cleanGameInstanceId(value) {
      const cleaned = cleanString(value);
      return cleaned && /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(cleaned)
        ? cleaned
        : null;
    }

    function cleanString(value) {
      const cleaned = String(value || "").trim();
      return cleaned.length > 0 ? cleaned : null;
    }

    tryInstall();
  }
})();
