const setupFlow = document.querySelector("#setup-flow");
const successPage = document.querySelector("#success-page");
const unsupportedPage = document.querySelector("#unsupported-page");
const extensionFinishPage = document.querySelector("#extension-finish");
const successTitle = document.querySelector("#success-title");
const successCopy = document.querySelector("#success-copy");
const browserField = document.querySelector("#browser-field");
const browserStatus = document.querySelector("#browser-status");
const browserSelect = document.querySelector("#browser");
const pathOutput = document.querySelector("#path");
const finishStatus = document.querySelector("#finish-status");
const browserHelpButton = document.querySelector("#browser-help");
const finishHelpButton = document.querySelector("#finish-help");
const finishSetupButton = document.querySelector("#finish-setup");
const openFolderButton = document.querySelector("#open-folder");
const copyPathButton = document.querySelector("#copy-path");
const backToSetupButton = document.querySelector("#back-to-setup");

const browsers = {
  brave: { label: "Brave" },
  chrome: { label: "Google Chrome" },
  edge: { label: "Microsoft Edge" },
  vivaldi: { label: "Vivaldi" },
  opera: { label: "Opera or Opera GX" },
  chromium: { label: "Chromium" }
};

const params = new URLSearchParams(window.location.search);
const hasExtensionRuntime =
  typeof chrome !== "undefined" && Boolean(chrome.runtime?.id);
const isExtensionSetupPage =
  hasExtensionRuntime && window.location.protocol === "chrome-extension:";
const folderPath = currentFolderPath();
let registrationStarted = false;
let connectionPollTimer = null;

pathOutput.textContent = folderPath;
initialize();

browserSelect.addEventListener("change", renderBrowserStatus);

browserHelpButton.addEventListener("click", () => {
  openHelpForBrowser(selectedBrowser());
});

finishHelpButton.addEventListener("click", async () => {
  openHelpForBrowser((await detectCurrentBrowser()) || "chrome");
});

finishSetupButton.addEventListener("click", finishExtensionSetup);

openFolderButton.addEventListener("click", () => {
  window.location.href = "./";
});

copyPathButton.addEventListener("click", async () => {
  await copyText(folderPath);
  pathOutput.textContent = `Copied folder path:\n${folderPath}`;
});

backToSetupButton.addEventListener("click", () => {
  clearConnectionPolling();
  successPage.hidden = true;
  unsupportedPage.hidden = true;
  extensionFinishPage.hidden = true;
  setupFlow.hidden = false;
});

async function initialize() {
  const detectedBrowser = await detectCurrentBrowser();

  if (isExtensionSetupPage) {
    showExtensionFinish(detectedBrowser);
    startConnectionPolling({ maxAttempts: 4, firstDelayMs: 250 });
    return;
  }

  const availableBrowsers = availableBrowserKeys(detectedBrowser);
  if (availableBrowsers.length === 0) {
    showUnsupportedBrowser();
    return;
  }

  renderBrowserOptions(availableBrowsers, detectedBrowser);

  if (params.get("connected") === "1") {
    showSuccess(
      "Voice Watch is connected",
      "The desktop app can already talk to the browser connector."
    );
  }
}

function showExtensionFinish(detectedBrowser) {
  setupFlow.hidden = true;
  successPage.hidden = true;
  unsupportedPage.hidden = true;
  extensionFinishPage.hidden = false;

  const browserLabel = detectedBrowser
    ? browsers[detectedBrowser]?.label || "this browser"
    : "this browser";
  finishStatus.textContent = `${browserLabel} is ready. Click Finish setup to connect it to the desktop app.`;
}

async function finishExtensionSetup() {
  if (!hasExtensionRuntime) {
    finishStatus.textContent = "Open this page from the installed browser extension.";
    return;
  }

  registrationStarted = true;
  finishSetupButton.disabled = true;
  finishStatus.textContent = "Opening Voice Watch to finish the desktop connection.";

  window.location.href = registrationLink(chrome.runtime.id, "all");

  window.setTimeout(() => {
    finishSetupButton.disabled = false;
    finishStatus.textContent = "Waiting for the desktop app to confirm the connection.";
  }, 1400);
  startConnectionPolling({ maxAttempts: 12, firstDelayMs: 1400 });
}

function startConnectionPolling({ maxAttempts, firstDelayMs }) {
  if (!hasExtensionRuntime || !chrome.runtime?.sendMessage) {
    return;
  }

  clearConnectionPolling();
  let attempts = 0;

  const tick = async () => {
    attempts += 1;
    const connected = await refreshExtensionConnection();
    if (connected || attempts >= maxAttempts) {
      return;
    }

    connectionPollTimer = window.setTimeout(tick, attempts < 3 ? 1000 : 2000);
  };

  connectionPollTimer = window.setTimeout(tick, firstDelayMs);
}

function clearConnectionPolling() {
  if (connectionPollTimer) {
    window.clearTimeout(connectionPollTimer);
    connectionPollTimer = null;
  }
}

async function refreshExtensionConnection() {
  try {
    const response = await chrome.runtime.sendMessage({ type: "get_status" });
    if (response?.nativeStatus?.connected) {
      showSuccess(
        "Voice Watch is connected",
        "The desktop app and browser connector are ready."
      );
      return true;
    }

    if (registrationStarted) {
      finishStatus.textContent =
        response?.nativeStatus?.lastError ||
        "Still waiting for Voice Watch. If Windows asks, choose Open Voice Watch.";
    }
  } catch (error) {
    if (registrationStarted) {
      finishStatus.textContent = error.message || String(error);
    }
  }

  return false;
}

function availableBrowserKeys(detectedBrowser) {
  const fromDesktop = params.has("browsers")
    ? params
        .get("browsers")
        .split(",")
        .map((value) => value.trim().toLowerCase())
        .filter((value) => browsers[value])
    : [];

  const values = fromDesktop.length > 0 ? fromDesktop : [];
  if (detectedBrowser && !values.includes(detectedBrowser)) {
    values.unshift(detectedBrowser);
  }

  return [...new Set(values)];
}

function renderBrowserOptions(availableBrowsers, detectedBrowser) {
  browserSelect.replaceChildren();
  for (const key of availableBrowsers) {
    const option = document.createElement("option");
    option.value = key;
    option.textContent = browsers[key].label;
    browserSelect.appendChild(option);
  }

  const preferred = preferredBrowser(availableBrowsers, detectedBrowser);
  if (preferred) {
    browserSelect.value = preferred;
  }

  renderBrowserStatus();
}

function renderBrowserStatus() {
  const selected = selectedBrowser();
  if (!selected) {
    browserStatus.textContent = "Choose the browser you will use for Roblox.";
    return;
  }

  browserStatus.textContent = `${browsers[selected].label} selected.`;
}

function preferredBrowser(availableBrowsers, detectedBrowser) {
  const requested = params.get("preferred");
  if (requested && availableBrowsers.includes(requested)) {
    return requested;
  }
  if (detectedBrowser && availableBrowsers.includes(detectedBrowser)) {
    return detectedBrowser;
  }
  return availableBrowsers[0] || "";
}

async function detectCurrentBrowser() {
  try {
    if (navigator.brave?.isBrave && (await navigator.brave.isBrave())) {
      return "brave";
    }
  } catch (_error) {
    // Some Chromium forks expose navigator.brave but reject the call.
  }

  const userAgent = navigator.userAgent;
  if (userAgent.includes("Edg/")) {
    return "edge";
  }
  if (userAgent.includes("OPR/") || userAgent.includes("Opera")) {
    return "opera";
  }
  if (userAgent.includes("Vivaldi") || window.vivaldi) {
    return "vivaldi";
  }
  if (userAgent.includes("Chromium")) {
    return "chromium";
  }
  if (userAgent.includes("Chrome/")) {
    return "chrome";
  }

  return "";
}

function showUnsupportedBrowser() {
  setupFlow.hidden = true;
  successPage.hidden = true;
  extensionFinishPage.hidden = true;
  unsupportedPage.hidden = false;
  browserField.hidden = true;
  browserHelpButton.disabled = true;
}

function showSuccess(title, copy) {
  clearConnectionPolling();
  successTitle.textContent = title;
  successCopy.textContent = copy;
  setupFlow.hidden = true;
  unsupportedPage.hidden = true;
  extensionFinishPage.hidden = true;
  successPage.hidden = false;
}

function openHelpForBrowser(browser) {
  if (!browser || !browsers[browser]) {
    return;
  }

  window.location.href = `help.html?browser=${encodeURIComponent(browser)}`;
}

function registrationLink(extensionId, browser) {
  return `voice-watch://register-native-host?extensionId=${extensionId}&browser=${encodeURIComponent(browser)}`;
}

function selectedBrowser() {
  return browserSelect.value && browsers[browserSelect.value] ? browserSelect.value : "";
}

function currentFolderPath() {
  if (window.location.protocol !== "file:") {
    return "Select the extension folder installed with Voice Watch.";
  }

  return decodeURIComponent(window.location.pathname.replace(/^\/([A-Za-z]:)/, "$1"))
    .replace(/\/setup\.html$/i, "")
    .replace(/\//g, "\\");
}

async function copyText(value) {
  if (navigator.clipboard?.writeText) {
    try {
      await navigator.clipboard.writeText(value);
      return;
    } catch (_error) {
      // Fall through to the textarea fallback used by local file pages.
    }
  }

  const textarea = document.createElement("textarea");
  textarea.value = value;
  textarea.setAttribute("readonly", "");
  textarea.style.position = "fixed";
  textarea.style.top = "-1000px";
  document.body.appendChild(textarea);
  textarea.select();
  document.execCommand("copy");
  textarea.remove();
}
