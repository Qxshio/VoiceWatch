const HOST_NAME = "com.voice_watch.native";
const PROTOCOL_VERSION = 1;
const VOICE_SETTINGS_URL = "https://voice.roblox.com/v1/settings";

let nativePort = null;
let nativeStatus = {
  connected: false,
  connecting: false,
  lastError: null,
  appVersion: null,
  pollIntervalSeconds: 10
};
let connectWaiters = [];

chrome.runtime.onInstalled.addListener(() => {
  chrome.storage.local.set({ nativeStatus });
});

chrome.runtime.onMessage.addListener((message, _sender, sendResponse) => {
  handleRuntimeMessage(message)
    .then(sendResponse)
    .catch((error) => {
      sendResponse({ ok: false, error: error.message || String(error) });
    });
  return true;
});

async function handleRuntimeMessage(message) {
  switch (message?.type) {
    case "connect_native":
      return connectNative();
    case "check_now": {
      const connection = await connectNative();
      if (!connection.ok) {
        return connection;
      }
      const requestId = crypto.randomUUID();
      const status = await fetchVoiceStatus(requestId);
      postNative(status);
      return { ok: true, status };
    }
    case "get_status":
      return { ok: true, nativeStatus };
    default:
      return { ok: false, error: "Unknown message type." };
  }
}

function connectNative() {
  if (nativePort) {
    if (nativeStatus.connected) {
      return Promise.resolve({ ok: true, nativeStatus });
    }

    return new Promise((resolve) => {
      connectWaiters.push(resolve);
    });
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
    chrome.storage.local.set({ nativeStatus });
    return Promise.resolve({ ok: false, nativeStatus });
  }

  nativePort.onMessage.addListener(handleNativeMessage);
  nativePort.onDisconnect.addListener(() => {
    const lastError = chrome.runtime.lastError?.message || "Native host disconnected.";
    nativeStatus = {
      ...nativeStatus,
      connecting: false,
      connected: false,
      lastError
    };
    nativePort = null;
    chrome.storage.local.set({ nativeStatus });
    resolveConnectWaiters({ ok: false, nativeStatus });
  });

  nativeStatus = {
    ...nativeStatus,
    connecting: true,
    connected: false,
    lastError: null
  };
  chrome.storage.local.set({ nativeStatus });

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
    chrome.storage.local.set({ nativeStatus });
    return Promise.resolve({ ok: false, nativeStatus });
  }

  return new Promise((resolve) => {
    connectWaiters.push(resolve);
    setTimeout(() => {
      if (nativeStatus.connected) {
        resolve({ ok: true, nativeStatus });
      } else {
        nativeStatus = {
          ...nativeStatus,
          connecting: false,
          connected: false,
          lastError: nativeStatus.lastError || "Timed out waiting for the desktop app."
        };
        chrome.storage.local.set({ nativeStatus });
        resolveConnectWaiters({ ok: false, nativeStatus });
      }
    }, 2500);
  });
}

function handleNativeMessage(message) {
  if (message?.type === "hello_ack") {
    nativeStatus = {
      connecting: false,
      connected: true,
      lastError: null,
      appVersion: message.appVersion,
      pollIntervalSeconds: message.pollIntervalSeconds ?? 10
    };
    chrome.storage.local.set({ nativeStatus });
    resolveConnectWaiters({ ok: true, nativeStatus });
    return;
  }

  if (message?.type === "check_voice_status") {
    fetchVoiceStatus(message.requestId)
      .then(postNative)
      .catch((error) => {
        postNative(errorEnvelope(message.requestId, "network_error", error.message));
      });
  }
}

function postNative(message) {
  if (!nativePort) {
    connectNative();
  }
  nativePort.postMessage(message);
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
