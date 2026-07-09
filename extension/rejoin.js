(() => {
  const params = new URLSearchParams(window.location.search);
  if (params.get("voiceWatchRejoin") !== "1") {
    return;
  }

  const payload = {
    placeId: numberString(params.get("placeId")) || placeIdFromPath(),
    gameInstanceId: clean(params.get("gameInstanceId")),
    accessCode: clean(params.get("accessCode")),
    linkCode: clean(params.get("linkCode"))
  };

  if (!payload.placeId) {
    return;
  }

  cleanupUrl(params);
  injectLauncher(payload);

  function injectLauncher(value) {
    const target = document.documentElement || document.head || document.body;
    if (!target) {
      window.addEventListener("DOMContentLoaded", () => injectLauncher(value), { once: true });
      return;
    }

    const script = document.createElement("script");
    script.textContent = `(${launchFromPageContext.toString()})(${JSON.stringify(value)});`;
    target.append(script);
    script.remove();
  }

  function cleanupUrl(urlParams) {
    for (const key of ["voiceWatchRejoin", "placeId", "gameInstanceId", "accessCode", "linkCode"]) {
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

  function clean(value) {
    const cleaned = String(value || "").trim();
    return cleaned.length > 0 ? cleaned : null;
  }

  function launchFromPageContext(rejoin) {
    const startedAt = Date.now();
    const timeoutMs = 30000;

    function tryLaunch() {
      const placeId = Number(rejoin.placeId);
      const launcher = window.Roblox && window.Roblox.GameLauncher;

      if (launcher && Number.isFinite(placeId)) {
        try {
          window._skipRobloxJoinInterceptor = true;
          if (rejoin.gameInstanceId && typeof launcher.joinGameInstance === "function") {
            launcher.joinGameInstance(placeId, rejoin.gameInstanceId);
            return;
          }

          const privateCode = rejoin.accessCode || rejoin.linkCode;
          if (privateCode && typeof launcher.joinPrivateGame === "function") {
            launcher.joinPrivateGame(placeId, privateCode);
            return;
          }
        } catch (error) {
          console.warn("[Voice Watch] Roblox launcher call failed, falling back.", error);
          fallbackToGameStart();
          return;
        }
      }

      if (Date.now() - startedAt < timeoutMs) {
        window.setTimeout(tryLaunch, 250);
        return;
      }

      fallbackToGameStart();
    }

    function fallbackToGameStart() {
      const query = new URLSearchParams({ placeId: String(rejoin.placeId) });
      if (rejoin.gameInstanceId) {
        query.set("gameInstanceId", rejoin.gameInstanceId);
      }
      if (rejoin.accessCode) {
        query.set("accessCode", rejoin.accessCode);
      }
      if (rejoin.linkCode) {
        query.set("linkCode", rejoin.linkCode);
      }

      window.location.assign(`https://www.roblox.com/games/start?${query.toString()}`);
    }

    tryLaunch();
  }
})();
