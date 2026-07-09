(() => {
  const script = document.currentScript;
  const payload = readPayload(script);
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

    fallbackToGameStart(rejoin);
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

  function readPayload(currentScript) {
    try {
      return JSON.parse(currentScript?.dataset?.voiceWatchPayload || "{}");
    } catch (_error) {
      return {};
    }
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
})();
