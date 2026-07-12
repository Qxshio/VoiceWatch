const HOST_NAME = "com.voice_watch.native";
const PROTOCOL_VERSION = 1;
const VOICE_SETTINGS_URL = "https://voice.roblox.com/v1/settings";
const AUTHENTICATED_USER_URL = "https://users.roblox.com/v1/users/authenticated";
const PRESENCE_URL = "https://presence.roblox.com/v1/presence/users";
const PRESENCE_REFRESH_MS = 60000;
const AUTHENTICATED_USER_CACHE_MS = 5 * 60 * 1000;

let nativePort = null;
let nativeStatus = {
  connected: false,
  connecting: false,
  lastError: null,
  appVersion: null,
  pollIntervalSeconds: 10,
  pollPausedReason: null,
  pollPausedMessage: null,
  robloxRunning: null,
  robloxPlaying: null,
  microphoneActive: null,
  robloxLoggedIn: null
};
let lastVoiceStatus = null;
let connectionAttempt = null;
let pollReadinessWaiters = new Map();
let pollTimer = null;
let pollInFlight = false;
let intentionalDisconnectPort = null;
let manuallyDisconnected = false;
let lastServer = null;
let currentUserId;
let authenticatedUserCheckedAtMs = 0;
let lastPresenceRefreshAtMs = 0;
let persistence = Promise.resolve();

const statusHydration = chrome.storage.local
  .get(["nativeStatus", "lastVoiceStatus", "manuallyDisconnected", "lastServer"])
  .then((stored) => {
    if (stored.nativeStatus) {
      nativeStatus = { ...nativeStatus, ...stored.nativeStatus, connected: false, connecting: false };
    }
    if (stored.lastVoiceStatus) {
      lastVoiceStatus = stored.lastVoiceStatus;
    }
    if (stored.lastServer) {
      lastServer = stored.lastServer;
    }
    manuallyDisconnected = Boolean(stored.manuallyDisconnected);
  });

chrome.runtime.onInstalled.addListener((details) => {
  ensureStatusHydrated().then(async () => {
    const connection = details?.reason === "install"
      ? await reconnectNative()
      : await connectNative();
    if (details?.reason === "install" && !connection.ok) {
      openSetupPage();
    }
  });
});

chrome.runtime.onStartup.addListener(() => {
  ensureStatusHydrated().then(() => {
    connectNative();
  });
});

chrome.runtime.onUpdateAvailable?.addListener(() => {
  chrome.runtime.reload();
});

chrome.runtime.onMessage.addListener((message, _sender, sendResponse) => {
  handleRuntimeMessage(message)
    .then(sendResponse)
    .catch((error) => {
      sendResponse({
        ok: false,
        error: error.message || String(error),
        nativeStatus,
        lastVoiceStatus
      });
    });
  return true;
});

async function handleRuntimeMessage(message) {
  await ensureStatusHydrated();

  switch (message?.type) {
    case "connect_native":
      return reconnectNative();
    case "disconnect_native":
      return disconnectNative();
    case "get_status": {
      const connection = manuallyDisconnected
        ? { ok: false, nativeStatus }
        : await connectNative();
      return {
        ok: connection.ok,
        nativeStatus,
        lastVoiceStatus,
        lastServer,
        manuallyDisconnected
      };
    }
    case "remember_last_server":
      return rememberLastServer(message.server);
    default:
      return {
        ok: false,
        error: "Unknown message type.",
        nativeStatus,
        lastVoiceStatus
      };
  }
}

async function ensureStatusHydrated() {
  await statusHydration;
}

function persistStatus() {
  const snapshot = {
    nativeStatus: { ...nativeStatus },
    lastVoiceStatus,
    manuallyDisconnected,
    lastServer
  };
  persistence = persistence
    .catch(() => {})
    .then(() => chrome.storage.local.set(snapshot));
  return persistence;
}

function openSetupPage() {
  try {
    chrome.tabs?.create?.({
      url: `${chrome.runtime.getURL("setup.html")}?from=extension`
    });
  } catch (_error) {
    // If a browser blocks tab creation here, the popup still offers Finish setup.
  }
}

function connectNative() {
  if (manuallyDisconnected) {
    return Promise.resolve({ ok: false, nativeStatus, manuallyDisconnected });
  }

  if (nativePort) {
    if (nativeStatus.connected) {
      startPolling(false);
      return Promise.resolve({ ok: true, nativeStatus, manuallyDisconnected });
    }

    if (connectionAttempt?.port === nativePort) {
      return connectionAttempt.promise;
    }

    const stalePort = nativePort;
    nativePort = null;
    try {
      stalePort.disconnect();
    } catch (_error) {
      // A stale port may already have been closed by the browser.
    }
  }

  let port;
  try {
    port = chrome.runtime.connectNative(HOST_NAME);
  } catch (error) {
    nativeStatus = {
      ...nativeStatus,
      connecting: false,
      connected: false,
      lastError: error.message || String(error)
    };
    persistStatus();
    return Promise.resolve({ ok: false, nativeStatus, manuallyDisconnected });
  }

  nativePort = port;
  port.onMessage.addListener((message) => handleNativeMessage(port, message));
  port.onDisconnect.addListener(() => handleNativeDisconnect(port));

  nativeStatus = {
    ...nativeStatus,
    connecting: true,
    connected: false,
    lastError: null
  };
  persistStatus();

  let resolveAttempt;
  const promise = new Promise((resolve) => {
    resolveAttempt = resolve;
  });
  const timeout = setTimeout(() => handleConnectionTimeout(port), 2500);
  connectionAttempt = { port, promise, resolve: resolveAttempt, timeout };

  try {
    port.postMessage({
      type: "hello",
      extensionVersion: chrome.runtime.getManifest().version,
      protocolVersion: PROTOCOL_VERSION
    });
  } catch (error) {
    nativeStatus = {
      ...nativeStatus,
      connecting: false,
      connected: false,
      lastError: error.message || String(error)
    };
    if (nativePort === port) {
      nativePort = null;
    }
    persistStatus();
    finishConnectionAttempt(port, {
      ok: false,
      nativeStatus,
      manuallyDisconnected
    });
    try {
      port.disconnect();
    } catch (_disconnectError) {
      // The failed post may already have closed the port.
    }
  }

  return promise;
}

async function reconnectNative() {
  manuallyDisconnected = false;
  nativeStatus = {
    ...nativeStatus,
    lastError: null
  };
  await persistStatus();
  return connectNative();
}

async function disconnectNative() {
  stopPolling();

  manuallyDisconnected = true;
  nativeStatus = {
    ...nativeStatus,
    connecting: false,
    connected: false,
    lastError: "Disconnected."
  };

  const port = nativePort;
  if (port) {
    intentionalDisconnectPort = port;
    try {
      port.postMessage({ type: "disconnect" });
    } catch (_error) {
      // The port may already be closing. The local status is still updated below.
    }

    nativePort = null;
    finishConnectionAttempt(port, {
      ok: false,
      nativeStatus,
      manuallyDisconnected
    });
    setTimeout(() => {
      try {
        port.disconnect();
      } catch (_error) {
        // Ignore duplicate disconnects from the browser runtime.
      }
    }, 50);
  }

  await persistStatus();
  resolvePollReadinessWaiters({
    shouldPoll: false,
    reason: "desktop_disconnected",
    message: "Desktop app disconnected."
  });
  return {
    ok: true,
    nativeStatus,
    lastVoiceStatus,
    manuallyDisconnected
  };
}

function handleNativeMessage(port, message) {
  if (nativePort !== port) {
    return;
  }

  if (message?.type === "hello_ack") {
    nativeStatus = {
      connecting: false,
      connected: true,
      lastError: null,
      appVersion: message.appVersion,
      pollIntervalSeconds: message.pollIntervalSeconds ?? 10,
      pollPausedReason: null,
      pollPausedMessage: null,
      robloxRunning: null,
      robloxPlaying: null,
      microphoneActive: null,
      robloxLoggedIn: null
    };
    persistStatus();
    finishConnectionAttempt(port, {
      ok: true,
      nativeStatus,
      manuallyDisconnected
    });
    if (lastServer) {
      postNative({ type: "last_server", server: lastServer });
    }
    if (lastVoiceStatus) {
      postNative(lastVoiceStatus);
    }
    startPolling(true);
    return;
  }

  if (message?.type === "poll_readiness") {
    rememberPollReadiness(message);
    resolvePollReadiness(message);
    return;
  }

  if (message?.type === "error") {
    nativeStatus = {
      ...nativeStatus,
      connecting: false,
      connected: false,
      lastError: message.message || "The desktop app rejected the connection."
    };
    persistStatus();
    finishConnectionAttempt(port, {
      ok: false,
      nativeStatus,
      manuallyDisconnected
    });
    if (nativePort === port) {
      nativePort = null;
    }
    try {
      port.disconnect();
    } catch (_error) {
      // The desktop may already have closed a rejected connection.
    }
    return;
  }

  if (message?.type === "check_voice_status") {
    fetchVoiceStatus(message.requestId)
      .then(rememberVoiceStatus)
      .then(postNative)
      .catch((error) => {
        const envelope = errorEnvelope(message.requestId, "network_error", error.message);
        rememberVoiceStatus(envelope);
        postNative(envelope);
      });
    return;
  }

  if (message?.type === "rejoin") {
    openRejoinInBrowser(message.server).catch((error) => {
      console.warn("[Voice Watch] Could not open the browser rejoin page.", error);
    });
    return;
  }

  if (message?.type === "update_extension") {
    requestExtensionUpdate().catch((error) => {
      console.warn("[Voice Watch] Could not request an extension update.", error);
    });
  }
}

async function requestExtensionUpdate() {
  // Firefox does not implement Chromium's manual store-update check.
  const requestUpdateCheck = Reflect.get(chrome.runtime, "requestUpdateCheck");
  if (typeof requestUpdateCheck === "function") {
    try {
      const result = await Reflect.apply(requestUpdateCheck, chrome.runtime, []);
      if (result?.status === "update_available") {
        return;
      }
    } catch (_error) {
      // Unpacked and some Firefox builds do not expose a store update check.
    }
  }

  chrome.runtime.reload();
}

function handleNativeDisconnect(port) {
  const intentional = intentionalDisconnectPort === port;
  if (intentional) {
    intentionalDisconnectPort = null;
  }

  const runtimeError = chrome.runtime.lastError?.message;
  if (nativePort !== port) {
    return;
  }

  stopPolling();
  resolvePollReadinessWaiters({
    shouldPoll: false,
    reason: "desktop_disconnected",
    message: "Desktop app disconnected."
  });
  nativeStatus = {
    ...nativeStatus,
    connecting: false,
    connected: false,
    lastError: intentional
      ? "Disconnected."
      : runtimeError || "Native host disconnected."
  };
  nativePort = null;
  persistStatus();
  finishConnectionAttempt(port, {
    ok: false,
    nativeStatus,
    manuallyDisconnected
  });
}

function handleConnectionTimeout(port) {
  if (connectionAttempt?.port !== port) {
    return;
  }

  nativeStatus = {
    ...nativeStatus,
    connecting: false,
    connected: false,
    lastError: nativeStatus.lastError || "Timed out waiting for the desktop app."
  };
  if (nativePort === port) {
    nativePort = null;
  }
  persistStatus();
  finishConnectionAttempt(port, {
    ok: false,
    nativeStatus,
    manuallyDisconnected
  });
  try {
    port.disconnect();
  } catch (_error) {
    // The browser may have closed the timed-out port already.
  }
}

function finishConnectionAttempt(port, result) {
  if (connectionAttempt?.port !== port) {
    return;
  }

  const attempt = connectionAttempt;
  connectionAttempt = null;
  clearTimeout(attempt.timeout);
  attempt.resolve(result);
}

function startPolling(immediate) {
  stopPolling();
  scheduleNextPoll(immediate ? 250 : pollIntervalMs());
}

function stopPolling() {
  if (pollTimer) {
    clearTimeout(pollTimer);
    pollTimer = null;
  }
}

function scheduleNextPoll(delayMs = pollIntervalMs()) {
  if (!nativeStatus.connected) {
    return;
  }

  stopPolling();
  pollTimer = setTimeout(pollVoiceStatus, nextPollDelayMs(delayMs));
}

async function pollVoiceStatus() {
  if (!nativeStatus.connected || pollInFlight) {
    scheduleNextPoll();
    return;
  }

  pollInFlight = true;
  try {
    const readiness = await requestPollReadiness();
    if (!readiness.shouldPoll) {
      return;
    }

    const envelope = await fetchVoiceStatus(newRequestId());
    await rememberVoiceStatus(envelope);
    postNative(envelope);
  } finally {
    pollInFlight = false;
    scheduleNextPoll();
  }
}

function requestPollReadiness() {
  if (!nativePort || !nativeStatus.connected) {
    return Promise.resolve({
      shouldPoll: false,
      reason: "desktop_disconnected",
      message: "Desktop app is not connected."
    });
  }

  const requestId = newRequestId();
  return new Promise((resolve) => {
    const timeout = setTimeout(() => {
      pollReadinessWaiters.delete(requestId);
      resolve({
        requestId,
        shouldPoll: false,
        reason: "readiness_timeout",
        message: "Desktop readiness check timed out."
      });
    }, 1500);

    pollReadinessWaiters.set(requestId, { resolve, timeout });

    try {
      nativePort.postMessage({
        type: "poll_readiness_request",
        requestId
      });
    } catch (_error) {
      clearTimeout(timeout);
      pollReadinessWaiters.delete(requestId);
      resolve({
        requestId,
        shouldPoll: false,
        reason: "readiness_unavailable",
        message: "Desktop readiness check was unavailable."
      });
    }
  });
}

function rememberPollReadiness(message) {
  const nextStatus = {
    ...nativeStatus,
    pollIntervalSeconds: message.pollIntervalSeconds ?? nativeStatus.pollIntervalSeconds,
    pollPausedReason: message.shouldPoll ? null : message.reason || "paused",
    pollPausedMessage: message.shouldPoll ? null : message.message || "Checks are paused.",
    robloxRunning: Boolean(message.robloxRunning),
    robloxPlaying: Boolean(message.robloxPlaying),
    microphoneActive: Boolean(message.microphoneActive)
  };
  const changed = !shallowEqual(nativeStatus, nextStatus);
  nativeStatus = nextStatus;
  if (changed) {
    persistStatus();
  }
  if (nativeStatus.robloxPlaying) {
    refreshCurrentPresence(false).catch(() => {});
  }
}

function resolvePollReadiness(message) {
  const waiter = pollReadinessWaiters.get(message.requestId);
  if (!waiter) {
    return;
  }

  clearTimeout(waiter.timeout);
  pollReadinessWaiters.delete(message.requestId);
  waiter.resolve(message);
}

function resolvePollReadinessWaiters(message) {
  const waiters = Array.from(pollReadinessWaiters.values());
  pollReadinessWaiters.clear();
  for (const waiter of waiters) {
    clearTimeout(waiter.timeout);
    waiter.resolve(message);
  }
}

function pollIntervalMs() {
  return Math.max(10, Number(nativeStatus.pollIntervalSeconds) || 10) * 1000;
}

function nextPollDelayMs(requestedDelayMs) {
  const suspensionDelay = suspensionPauseDelayMs();
  if (suspensionDelay !== null) {
    return suspensionDelay;
  }

  const rateLimitDelay = rateLimitPauseDelayMs();
  return rateLimitDelay === null
    ? requestedDelayMs
    : Math.max(requestedDelayMs, rateLimitDelay);
}

function suspensionPauseDelayMs() {
  const data = lastVoiceStatus?.ok ? lastVoiceStatus.data : null;
  if (!data?.isBanned || !Number.isFinite(data.bannedUntilMs)) {
    return null;
  }

  const remainingMs = data.bannedUntilMs - Date.now();
  if (remainingMs <= 0) {
    return null;
  }

  return Math.max(1000, remainingMs + 1000);
}

function rateLimitPauseDelayMs() {
  if (lastVoiceStatus?.ok || lastVoiceStatus?.error?.kind !== "rate_limited") {
    return null;
  }

  const retryAfterMs = Number(lastVoiceStatus.error.retryAfterMs);
  const checkedAt = Number(lastVoiceStatus.checkedAt);
  if (!Number.isFinite(retryAfterMs) || !Number.isFinite(checkedAt)) {
    return null;
  }

  const remainingMs = checkedAt + retryAfterMs - Date.now();
  return remainingMs > 0 ? remainingMs + 1000 : null;
}

async function rememberVoiceStatus(envelope) {
  lastVoiceStatus = envelope;
  await persistStatus();
  if (envelope?.ok && envelope.data?.isBanned && nativeStatus.robloxPlaying) {
    refreshCurrentPresence(true).catch(() => {});
  }
  return envelope;
}

async function rememberLastServer(server) {
  const normalized = normalizeServer(server);
  if (!normalized) {
    return { ok: false, error: "Server metadata was incomplete.", nativeStatus, lastVoiceStatus, lastServer };
  }

  if (lastServer && sameServer(lastServer, normalized)) {
    return { ok: true, nativeStatus, lastVoiceStatus, lastServer };
  }

  if (lastServer && normalized.detectedAtMs < (Number(lastServer.detectedAtMs) || 0)) {
    return { ok: true, nativeStatus, lastVoiceStatus, lastServer };
  }

  lastServer = normalized;
  await persistStatus();
  postNative({ type: "last_server", server: lastServer });
  return { ok: true, nativeStatus, lastVoiceStatus, lastServer };
}

async function refreshCurrentPresence(force) {
  if (!nativeStatus.connected && !force) {
    return { ok: false, skipped: true };
  }

  const now = Date.now();
  if (!force && now - lastPresenceRefreshAtMs < PRESENCE_REFRESH_MS) {
    return { ok: true, skipped: true, lastServer };
  }
  lastPresenceRefreshAtMs = now;

  const userId = await authenticatedUserId();
  if (userId === null) {
    await rememberLoggedOut();
    return { ok: false, error: "Roblox user is not signed in." };
  }
  if (!userId) {
    return { ok: false, error: "Roblox login status could not be confirmed." };
  }

  const response = await fetchPresence(userId);
  if (response.status === 401 || response.status === 403) {
    currentUserId = null;
    await rememberLoggedOut();
    return { ok: false, error: "Roblox user is not signed in." };
  }
  if (!response.ok) {
    return { ok: false, error: `Roblox presence returned HTTP ${response.status}.` };
  }

  const body = await response.json();
  const presence = Array.isArray(body?.userPresences) ? body.userPresences[0] : null;
  const server = serverFromPresence(presence);
  if (!server) {
    return { ok: false, error: "Roblox presence did not include a joinable game." };
  }

  return rememberLastServer(server);
}

async function fetchPresence(userId) {
  let response = await fetchPresenceWithHeaders(userId, {});
  const csrfToken = response.status === 403 ? response.headers.get("x-csrf-token") : null;
  if (csrfToken) {
    response = await fetchPresenceWithHeaders(userId, { "X-CSRF-TOKEN": csrfToken });
  }
  return response;
}

function fetchPresenceWithHeaders(userId, extraHeaders) {
  return fetch(PRESENCE_URL, {
    method: "POST",
    credentials: "include",
    cache: "no-store",
    headers: {
      "Accept": "application/json",
      "Content-Type": "application/json",
      ...extraHeaders
    },
    body: JSON.stringify({ userIds: [userId] })
  });
}

async function authenticatedUserId() {
  const now = Date.now();
  if (
    currentUserId !== undefined &&
    now - authenticatedUserCheckedAtMs < AUTHENTICATED_USER_CACHE_MS
  ) {
    return currentUserId;
  }

  let response;
  try {
    response = await fetch(AUTHENTICATED_USER_URL, {
      credentials: "include",
      cache: "no-store",
      headers: {
        "Accept": "application/json"
      }
    });
  } catch (_error) {
    return currentUserId || undefined;
  }
  if (response.status === 401 || response.status === 403) {
    currentUserId = null;
    authenticatedUserCheckedAtMs = now;
    return null;
  }
  if (!response.ok) {
    return currentUserId || undefined;
  }

  const body = await response.json();
  const id = optionalInteger(body?.id);
  if (id) {
    currentUserId = id;
    authenticatedUserCheckedAtMs = now;
    if (nativeStatus.robloxLoggedIn !== true) {
      nativeStatus = { ...nativeStatus, robloxLoggedIn: true };
      await persistStatus();
    }
  }
  return currentUserId;
}

async function rememberLoggedOut() {
  nativeStatus = { ...nativeStatus, robloxLoggedIn: false };
  const envelope = errorEnvelope(
    newRequestId(),
    "auth_error",
    "Please log in to Roblox in this browser."
  );
  await rememberVoiceStatus(envelope);
  postNative(envelope);
}

function serverFromPresence(presence) {
  const placeId = optionalInteger(presence?.placeId) || optionalInteger(presence?.rootPlaceId);
  const gameInstanceId = cleanGameInstanceId(presence?.gameId);
  if (!placeId || !gameInstanceId) {
    return null;
  }

  return {
    placeId,
    gameInstanceId,
    detectedAtMs: Date.now()
  };
}

function normalizeServer(server) {
  const placeId = optionalInteger(server?.placeId);
  const gameInstanceId = cleanGameInstanceId(server?.gameInstanceId);
  const accessCode = cleanString(server?.accessCode);
  const linkCode = cleanString(server?.linkCode);
  if (!placeId || (!gameInstanceId && !accessCode && !linkCode)) {
    return null;
  }

  return {
    placeId,
    gameInstanceId,
    accessCode,
    linkCode,
    detectedAtMs: optionalInteger(server?.detectedAtMs) || Date.now()
  };
}

async function openRejoinInBrowser(server) {
  const normalized = normalizeServer(server);
  if (!normalized) {
    throw new Error("Exact server metadata is unavailable.");
  }

  const query = new URLSearchParams({
    voiceWatchRejoin: "1",
    placeId: String(normalized.placeId)
  });
  if (normalized.gameInstanceId) {
    query.set("gameInstanceId", normalized.gameInstanceId);
  }
  if (normalized.accessCode) {
    query.set("accessCode", normalized.accessCode);
  }
  if (normalized.linkCode) {
    query.set("linkCode", normalized.linkCode);
  }

  await chrome.tabs.create({
    url: `https://www.roblox.com/games/${normalized.placeId}/Voice-Watch?${query.toString()}`
  });
}

function sameServer(left, right) {
  return left.placeId === right.placeId &&
    (left.gameInstanceId || null) === (right.gameInstanceId || null) &&
    (left.accessCode || null) === (right.accessCode || null) &&
    (left.linkCode || null) === (right.linkCode || null);
}

function shallowEqual(left, right) {
  const keys = Object.keys(right);
  return keys.length === Object.keys(left).length &&
    keys.every((key) => left[key] === right[key]);
}

function optionalInteger(value) {
  if (value === null || value === undefined || value === "") {
    return null;
  }

  const number = Number(value);
  return Number.isFinite(number) ? Math.trunc(number) : null;
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

function postNative(message) {
  const port = nativePort;
  if (!port) {
    return;
  }

  try {
    port.postMessage(message);
  } catch (error) {
    if (nativePort === port) {
      nativePort = null;
    }
    nativeStatus = {
      ...nativeStatus,
      connected: false,
      connecting: false,
      lastError: error.message || String(error)
    };
    stopPolling();
    persistStatus();
    finishConnectionAttempt(port, {
      ok: false,
      nativeStatus,
      manuallyDisconnected
    });
    try {
      port.disconnect();
    } catch (_disconnectError) {
      // The failed post may already have closed the port.
    }
  }
}

async function fetchVoiceStatus(requestId) {
  const checkedAt = Date.now();

  let response;
  try {
    response = await fetch(VOICE_SETTINGS_URL, {
      credentials: "include",
      cache: "no-store",
      headers: {
        "Accept": "application/json"
      }
    });
  } catch (error) {
    return errorEnvelope(requestId, "network_error", error.message, checkedAt);
  }

  if (response.status === 401 || response.status === 403) {
    currentUserId = null;
    authenticatedUserCheckedAtMs = checkedAt;
    nativeStatus = { ...nativeStatus, robloxLoggedIn: false };
    return errorEnvelope(
      requestId,
      "auth_error",
      "Please log in to Roblox in this browser.",
      checkedAt
    );
  }

  if (response.status === 429) {
    return errorEnvelope(
      requestId,
      "rate_limited",
      "Roblox rate limited the status check.",
      checkedAt,
      retryAfterMs(response)
    );
  }

  if (!response.ok) {
    return errorEnvelope(
      requestId,
      "network_error",
      `Roblox voice status request failed with HTTP ${response.status}.`,
      checkedAt
    );
  }

  if (nativeStatus.robloxLoggedIn !== true) {
    if (currentUserId === null) {
      currentUserId = undefined;
      authenticatedUserCheckedAtMs = 0;
      lastPresenceRefreshAtMs = 0;
    }
    nativeStatus = { ...nativeStatus, robloxLoggedIn: true };
  }

  let body;
  try {
    body = await response.json();
  } catch (_error) {
    return errorEnvelope(
      requestId,
      "unexpected_response",
      "Roblox returned a response that was not valid JSON.",
      checkedAt
    );
  }

  return {
    type: "voice_status",
    requestId,
    checkedAt,
    ok: true,
    data: sanitizeVoiceStatus(body)
  };
}

function sanitizeVoiceStatus(body) {
  return {
    isVoiceEnabled: Boolean(body?.isVoiceEnabled),
    isUserOptIn: Boolean(body?.isUserOptIn),
    isUserEligible: Boolean(body?.isUserEligible),
    isBanned: Boolean(body?.isBanned),
    banReason: asOptionalInteger(body?.banReason),
    bannedUntilMs: bannedUntilToMs(body?.bannedUntil),
    denialReason: asOptionalInteger(body?.denialReason)
  };
}

function bannedUntilToMs(bannedUntil) {
  if (!bannedUntil || bannedUntil.Seconds === undefined) {
    return null;
  }

  const seconds = Number(bannedUntil.Seconds);
  const nanos = Number(bannedUntil.Nanos ?? 0);
  if (!Number.isFinite(seconds) || !Number.isFinite(nanos)) {
    return null;
  }

  return Math.trunc(seconds * 1000 + nanos / 1_000_000);
}

function asOptionalInteger(value) {
  if (value === null || value === undefined) {
    return null;
  }

  const number = Number(value);
  return Number.isFinite(number) ? Math.trunc(number) : null;
}

function retryAfterMs(response) {
  const retryAfter = response.headers.get("Retry-After");
  if (!retryAfter) {
    return null;
  }

  const seconds = Number(retryAfter);
  return Number.isFinite(seconds) ? Math.max(0, Math.trunc(seconds * 1000)) : null;
}

function errorEnvelope(requestId, kind, message, checkedAt = Date.now(), retryAfter = null) {
  return {
    type: "voice_status",
    requestId,
    checkedAt,
    ok: false,
    error: {
      kind,
      message,
      retryAfterMs: retryAfter
    }
  };
}

function newRequestId() {
  if (crypto.randomUUID) {
    return crypto.randomUUID();
  }

  return `${Date.now()}-${Math.random().toString(16).slice(2)}`;
}
