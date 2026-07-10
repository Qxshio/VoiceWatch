const HOST_NAME = "com.voice_watch.native";
const PROTOCOL_VERSION = 1;
const VOICE_SETTINGS_URL = "https://voice.roblox.com/v1/settings";
const AUTHENTICATED_USER_URL = "https://users.roblox.com/v1/users/authenticated";
const PRESENCE_URL = "https://presence.roblox.com/v1/presence/users";
const PRESENCE_REFRESH_MS = 15000;

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
let connectWaiters = [];
let pollReadinessWaiters = new Map();
let pollTimer = null;
let pollInFlight = false;
let presenceTimer = null;
let intentionalDisconnect = false;
let manuallyDisconnected = false;
let lastServer = null;
let currentUserId = null;
let lastPresenceRefreshAtMs = 0;

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
    manuallyDisconnected = false;
    await persistStatus();
    const connection = await connectNative();
    if (details?.reason === "install" && !connection.ok) {
      openSetupPage();
    }
  });
});

chrome.runtime.onStartup.addListener(() => {
  ensureStatusHydrated().then(() => {
    manuallyDisconnected = false;
    persistStatus();
    connectNative();
  });
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
    case "disconnect_native":
      return disconnectNative();
    case "get_status": {
      const connection = manuallyDisconnected
        ? { ok: false, nativeStatus }
        : await connectNative();
      refreshCurrentPresence(false).catch(() => {});
      return {
        ok: connection.ok,
        nativeStatus,
        lastVoiceStatus,
        lastServer
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
  return chrome.storage.local.set({ nativeStatus, lastVoiceStatus, manuallyDisconnected, lastServer });
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
    return Promise.resolve({ ok: false, nativeStatus });
  }

  if (nativePort) {
    if (nativeStatus.connected) {
      startPolling(false);
      return Promise.resolve({ ok: true, nativeStatus });
    }

    return waitForConnection();
  }

  try {
    nativePort = chrome.runtime.connectNative(HOST_NAME);
  } catch (error) {
    nativeStatus = {
      ...nativeStatus,
      connecting: false,
      connected: false,
      lastError: error.message || String(error)
    };
    persistStatus();
    return Promise.resolve({ ok: false, nativeStatus });
  }

  nativePort.onMessage.addListener(handleNativeMessage);
  nativePort.onDisconnect.addListener(() => {
    stopPolling();
    stopPresenceRefresh();
    resolvePollReadinessWaiters({
      shouldPoll: false,
      reason: "desktop_disconnected",
      message: "Desktop app disconnected."
    });
    const lastError = intentionalDisconnect
      ? "Disconnected."
      : chrome.runtime.lastError?.message || "Native host disconnected.";
    intentionalDisconnect = false;
    nativeStatus = {
      ...nativeStatus,
      connecting: false,
      connected: false,
      lastError
    };
    nativePort = null;
    persistStatus();
    resolveConnectWaiters({ ok: false, nativeStatus });
  });

  nativeStatus = {
    ...nativeStatus,
    connecting: true,
    connected: false,
    lastError: null
  };
  persistStatus();

  try {
    nativePort.postMessage({
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
    nativePort = null;
    persistStatus();
    return Promise.resolve({ ok: false, nativeStatus });
  }

  return waitForConnection();
}

function waitForConnection() {
  return new Promise((resolve) => {
    connectWaiters.push(resolve);
    setTimeout(() => {
      if (nativeStatus.connected) {
        resolve({ ok: true, nativeStatus });
        return;
      }

      nativeStatus = {
        ...nativeStatus,
        connecting: false,
        connected: false,
        lastError: nativeStatus.lastError || "Timed out waiting for the desktop app."
      };
      persistStatus();
      resolveConnectWaiters({ ok: false, nativeStatus });
    }, 2500);
  });
}

function disconnectNative() {
  stopPolling();
  stopPresenceRefresh();

  if (nativePort) {
    intentionalDisconnect = true;
    try {
      nativePort.postMessage({ type: "disconnect" });
    } catch (_error) {
      // The port may already be closing. The local status is still updated below.
    }

    const port = nativePort;
    nativePort = null;
    setTimeout(() => {
      try {
        port.disconnect();
      } catch (_error) {
        // Ignore duplicate disconnects from the browser runtime.
      }
    }, 50);
  }

  manuallyDisconnected = true;
  nativeStatus = {
    ...nativeStatus,
    connecting: false,
    connected: false,
    lastError: "Disconnected."
  };
  persistStatus();
  resolveConnectWaiters({ ok: false, nativeStatus });
  resolvePollReadinessWaiters({
    shouldPoll: false,
    reason: "desktop_disconnected",
    message: "Desktop app disconnected."
  });
  return Promise.resolve({ ok: true, nativeStatus, lastVoiceStatus });
}

function handleNativeMessage(message) {
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
    resolveConnectWaiters({ ok: true, nativeStatus });
    if (lastServer) {
      postNative({ type: "last_server", server: lastServer });
    }
    startPresenceRefresh(true);
    startPolling(true);
    return;
  }

  if (message?.type === "poll_readiness") {
    rememberPollReadiness(message);
    resolvePollReadiness(message);
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
  }
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

function startPresenceRefresh(immediate) {
  stopPresenceRefresh();
  if (immediate) {
    refreshCurrentPresence(true).catch(() => {});
  }
  presenceTimer = setInterval(() => {
    refreshCurrentPresence(false).catch(() => {});
  }, PRESENCE_REFRESH_MS);
}

function stopPresenceRefresh() {
  if (presenceTimer) {
    clearInterval(presenceTimer);
    presenceTimer = null;
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
        shouldPoll: true,
        reason: null,
        message: null
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
        shouldPoll: true,
        reason: null,
        message: null
      });
    }
  });
}

function rememberPollReadiness(message) {
  nativeStatus = {
    ...nativeStatus,
    pollIntervalSeconds: message.pollIntervalSeconds ?? nativeStatus.pollIntervalSeconds,
    pollPausedReason: message.shouldPoll ? null : message.reason || "paused",
    pollPausedMessage: message.shouldPoll ? null : message.message || "Checks are paused.",
    robloxRunning: Boolean(message.robloxRunning),
    robloxPlaying: Boolean(message.robloxPlaying),
    microphoneActive: Boolean(message.microphoneActive)
  };
  persistStatus();
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
  return suspensionDelay === null ? requestedDelayMs : suspensionDelay;
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

async function rememberVoiceStatus(envelope) {
  lastVoiceStatus = envelope;
  await persistStatus();
  if (envelope?.ok && envelope.data?.isBanned) {
    refreshCurrentPresence(true).catch(() => {});
  }
  return envelope;
}

async function rememberLastServer(server) {
  const normalized = normalizeServer(server);
  if (!normalized) {
    return { ok: false, error: "Server metadata was incomplete.", nativeStatus, lastVoiceStatus, lastServer };
  }

  if (!lastServer || normalized.detectedAtMs >= (Number(lastServer.detectedAtMs) || 0)) {
    lastServer = normalized;
  }
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
    return null;
  }
  if (!response.ok) {
    return currentUserId || undefined;
  }

  const body = await response.json();
  const id = optionalInteger(body?.id);
  if (id) {
    currentUserId = id;
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
  return persistStatus();
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
  if (!nativePort) {
    return;
  }

  try {
    nativePort.postMessage(message);
  } catch (error) {
    nativeStatus = {
      ...nativeStatus,
      connected: false,
      connecting: false,
      lastError: error.message || String(error)
    };
    stopPolling();
    persistStatus();
  }
}

function resolveConnectWaiters(result) {
  const waiters = connectWaiters.splice(0);
  for (const resolve of waiters) {
    resolve(result);
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
    nativeStatus = { ...nativeStatus, robloxLoggedIn: false };
    await persistStatus();
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
    nativeStatus = { ...nativeStatus, robloxLoggedIn: true };
    await persistStatus();
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
