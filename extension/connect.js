const connection = document.querySelector("#connection");
const result = document.querySelector("#result");
const desktopStatus = document.querySelector("#desktop-status");
const desktopDetail = document.querySelector("#desktop-detail");
const voiceStatus = document.querySelector("#voice-status");
const voiceDetail = document.querySelector("#voice-detail");
const lastChecked = document.querySelector("#last-checked");
const lastDetail = document.querySelector("#last-detail");
const finishSetupButton = document.querySelector("#finish-setup");
const disconnectButton = document.querySelector("#disconnect");

const hasExtensionRuntime =
  typeof chrome !== "undefined" && chrome.runtime?.sendMessage;

if (hasExtensionRuntime) {
  finishSetupButton.addEventListener("click", finishSetup);
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
  finishSetupButton.hidden = true;
}

async function refreshStatus() {
  try {
    const response = await chrome.runtime.sendMessage({ type: "get_status" });
    renderStatus(response);
  } catch (error) {
    connection.textContent = "Cannot reach extension service";
    desktopStatus.textContent = "Error";
    desktopDetail.textContent = error.message || String(error);
    finishSetupButton.hidden = true;
    disconnectButton.hidden = true;
  }
}

async function finishSetup() {
  finishSetupButton.disabled = true;
  result.textContent = "Opening Voice Watch setup...";

  try {
    window.location.href = registrationLink(chrome.runtime.id, "all");

    window.setTimeout(() => {
      refreshStatus();
      finishSetupButton.disabled = false;
    }, 1600);
  } catch (error) {
    result.textContent = error.message || String(error);
    finishSetupButton.disabled = false;
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
      nativeStatus.robloxLoggedIn === false
        ? "Desktop bridge is ready. Roblox is logged out in this browser."
        : nativeStatus.pollPausedMessage ||
          `Checking about every ${nativeStatus.pollIntervalSeconds ?? 10} seconds.`;
    finishSetupButton.hidden = true;
    disconnectButton.hidden = false;
  } else if (nativeStatus?.connecting) {
    connection.textContent = "Connecting to desktop app...";
    desktopStatus.textContent = "Connecting";
    desktopDetail.textContent = "Waiting for Voice Watch to reply.";
    finishSetupButton.hidden = true;
    disconnectButton.hidden = true;
  } else {
    connection.textContent = "Desktop not connected";
    desktopStatus.textContent = "Disconnected";
    desktopDetail.textContent =
      nativeStatus?.lastError || "Click Finish setup to connect this browser.";
    finishSetupButton.hidden = false;
    disconnectButton.hidden = true;
  }

  renderVoiceStatus(lastVoiceStatus, nativeStatus);
}

function renderVoiceStatus(envelope, nativeStatus) {
  if (isLoggedOut(envelope, nativeStatus)) {
    voiceStatus.textContent = "Logged out";
    voiceDetail.textContent = "Log in to Roblox in this browser, then Voice Watch will resume checks.";
    lastChecked.textContent = Number.isFinite(envelope?.checkedAt)
      ? formatDateTime(envelope.checkedAt)
      : "--";
    lastDetail.textContent = Number.isFinite(envelope?.checkedAt)
      ? "Roblox account check completed."
      : "Waiting for Roblox login.";
    return;
  }

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
    const kind = envelope.error?.kind;
    voiceStatus.textContent = kind === "rate_limited" ? "Rate limited" : "Needs attention";
    voiceDetail.textContent =
      kind === "rate_limited"
        ? "Roblox asked Voice Watch to slow down briefly."
        : envelope.error?.message || "Voice status check failed.";
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

function isLoggedOut(envelope, nativeStatus) {
  return nativeStatus?.robloxLoggedIn === false || envelope?.error?.kind === "auth_error";
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

function registrationLink(extensionId, browser) {
  return `voice-watch://register-native-host?extensionId=${extensionId}&browser=${encodeURIComponent(browser)}`;
}
