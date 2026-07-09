const connection = document.querySelector("#connection");
const result = document.querySelector("#result");
const desktopStatus = document.querySelector("#desktop-status");
const desktopDetail = document.querySelector("#desktop-detail");
const voiceStatus = document.querySelector("#voice-status");
const voiceDetail = document.querySelector("#voice-detail");
const lastChecked = document.querySelector("#last-checked");
const lastDetail = document.querySelector("#last-detail");
const disconnectButton = document.querySelector("#disconnect");

const hasExtensionRuntime =
  typeof chrome !== "undefined" && chrome.runtime?.sendMessage;

if (hasExtensionRuntime) {
  disconnectButton.addEventListener("click", disconnect);
  refreshStatus();
  setInterval(refreshStatus, 2000);
} else {
  connection.textContent = "Open from the browser extension";
  desktopStatus.textContent = "Unavailable";
  desktopDetail.textContent =
    "Load the Voice Watch extension in your browser before opening this popup.";
  voiceStatus.textContent = "Unknown";
  voiceDetail.textContent = "This page cannot read extension status from a normal tab.";
  lastChecked.textContent = "--";
  lastDetail.textContent = "Open setup.html for install steps.";
}

async function refreshStatus() {
  try {
    const response = await chrome.runtime.sendMessage({ type: "get_status" });
    renderStatus(response);
  } catch (error) {
    connection.textContent = "Cannot reach extension service";
    desktopStatus.textContent = "Error";
    desktopDetail.textContent = error.message || String(error);
    disconnectButton.hidden = true;
  }
}

async function disconnect() {
  disconnectButton.disabled = true;
  result.textContent = "Disconnecting...";

  try {
    const response = await chrome.runtime.sendMessage({ type: "disconnect_native" });
    renderStatus(response);
    result.textContent = "Disconnected.";
  } catch (error) {
    result.textContent = error.message || String(error);
  } finally {
    disconnectButton.disabled = false;
  }
}

function renderStatus(response) {
  const nativeStatus = response?.nativeStatus;
  const lastVoiceStatus = response?.lastVoiceStatus;

  if (nativeStatus?.connected) {
    connection.textContent = "Desktop connected";
    desktopStatus.textContent = nativeStatus.appVersion
      ? `Voice Watch ${nativeStatus.appVersion}`
      : "Connected";
    desktopDetail.textContent =
      nativeStatus.pollPausedMessage ||
      `Checking about every ${nativeStatus.pollIntervalSeconds ?? 10} seconds.`;
    disconnectButton.hidden = false;
  } else if (nativeStatus?.connecting) {
    connection.textContent = "Connecting to desktop app...";
    desktopStatus.textContent = "Connecting";
    desktopDetail.textContent = "Waiting for Voice Watch to reply.";
    disconnectButton.hidden = true;
  } else {
    connection.textContent = "Desktop not connected";
    desktopStatus.textContent = "Disconnected";
    desktopDetail.textContent =
      nativeStatus?.lastError || "Open Voice Watch from the tray to finish setup.";
    disconnectButton.hidden = true;
  }

  renderVoiceStatus(lastVoiceStatus, nativeStatus);
}

function renderVoiceStatus(envelope, nativeStatus) {
  if (nativeStatus?.microphoneActive) {
    voiceStatus.textContent = "Active";
    voiceDetail.textContent = "Roblox is using your microphone, so web checks are paused.";
    if (!envelope) {
      lastChecked.textContent = "--";
      lastDetail.textContent = "Using local microphone activity.";
    }
    return;
  }

  if (nativeStatus?.pollPausedReason === "roblox_not_playing" && !envelope) {
    voiceStatus.textContent = "Waiting";
    voiceDetail.textContent = "Open a Roblox game to start checks.";
    lastChecked.textContent = "--";
    lastDetail.textContent = "No game window detected.";
    return;
  }

  if (!envelope) {
    voiceStatus.textContent = "Unknown";
    voiceDetail.textContent = "Waiting for the first status check.";
    lastChecked.textContent = "--";
    lastDetail.textContent = "No result yet.";
    return;
  }

  lastChecked.textContent = formatDateTime(envelope.checkedAt);
  lastDetail.textContent = envelope.ok ? "Roblox replied." : "Check did not complete.";

  if (!envelope.ok) {
    voiceStatus.textContent = "Needs attention";
    voiceDetail.textContent = envelope.error?.message || "Voice status check failed.";
    return;
  }

  const data = envelope.data || {};
  if (data.isBanned) {
    voiceStatus.textContent = "Banned";
    voiceDetail.textContent = data.bannedUntilMs
      ? `Suspension ends ${formatDateTime(data.bannedUntilMs)}.`
      : "Suspension duration is unknown.";
    return;
  }

  if (data.isVoiceEnabled && data.isUserOptIn && data.isUserEligible) {
    voiceStatus.textContent = "Unbanned";
    voiceDetail.textContent = "Voice chat appears available.";
    return;
  }

  voiceStatus.textContent = "Unavailable";
  voiceDetail.textContent = "Roblox says voice chat is not available for this session.";
}

function formatDateTime(value) {
  if (!Number.isFinite(value)) {
    return "--";
  }

  return new Date(value).toLocaleString([], {
    month: "short",
    day: "numeric",
    hour: "numeric",
    minute: "2-digit"
  });
}
