const setupFlow = document.querySelector("#setup-flow");
const successPage = document.querySelector("#success-page");
const unsupportedPage = document.querySelector("#unsupported-page");
const successTitle = document.querySelector("#success-title");
const successCopy = document.querySelector("#success-copy");
const browserField = document.querySelector("#browser-field");
const browserStatus = document.querySelector("#browser-status");
const browserSelect = document.querySelector("#browser");
const pathOutput = document.querySelector("#path");
const registerLinkOutput = document.querySelector("#register-link");
const extensionIdInput = document.querySelector("#extension-id");
const idStatus = document.querySelector("#id-status");
const browserHelpButton = document.querySelector("#browser-help");
const openFolderButton = document.querySelector("#open-folder");
const copyPathButton = document.querySelector("#copy-path");
const registerButton = document.querySelector("#register");
const backToSetupButton = document.querySelector("#back-to-setup");

const browsers = {
  brave: { label: "Brave" },
  chrome: { label: "Google Chrome" },
  edge: { label: "Microsoft Edge" },
  vivaldi: { label: "Vivaldi" },
  opera: { label: "Opera" },
  chromium: { label: "Chromium" }
};

const params = new URLSearchParams(window.location.search);
const folderPath = decodeURIComponent(
  window.location.pathname.replace(/^\/([A-Za-z]:)/, "$1")
)
  .replace(/\/setup\.html$/i, "")
  .replace(/\//g, "\\");

pathOutput.textContent = folderPath;
initialize();

browserSelect.addEventListener("change", renderRegistration);
extensionIdInput.addEventListener("input", renderRegistration);

browserHelpButton.addEventListener("click", () => {
  const browser = selectedBrowser();
  if (!browser) {
    return;
  }

  window.location.href = `help.html?browser=${encodeURIComponent(browser)}`;
});

openFolderButton.addEventListener("click", () => {
  window.location.href = "./";
});

copyPathButton.addEventListener("click", async () => {
  await copyText(folderPath);
  pathOutput.textContent = `Copied folder path:\n${folderPath}`;
});

registerButton.addEventListener("click", () => {
  const extensionId = sanitizeExtensionId(extensionIdInput.value);
  const browser = selectedBrowser();
  if (!extensionId || !browser) {
    renderRegistration();
    return;
  }

  const link = registrationLink(extensionId, browser);
  registerLinkOutput.textContent = `Opening Voice Watch setup:\n${link}`;
  window.location.href = link;
  setTimeout(() => {
    showSuccess(
      "Desktop connection registered",
      "Voice Watch accepted the browser connector details."
    );
  }, 700);
});

backToSetupButton.addEventListener("click", () => {
  successPage.hidden = true;
  unsupportedPage.hidden = true;
  setupFlow.hidden = false;
});

async function initialize() {
  const detectedBrowser = await detectCurrentBrowser();
  const availableBrowsers = availableBrowserKeys(detectedBrowser);

  if (availableBrowsers.length === 0) {
    showUnsupportedBrowser();
    return;
  }

  renderBrowserOptions(availableBrowsers, detectedBrowser);
  renderRegistration();

  if (params.get("connected") === "1") {
    showSuccess(
      "Voice Watch is connected",
      "The desktop app can already talk to the browser connector."
    );
  }
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

  if (availableBrowsers.length === 1) {
    browserStatus.textContent = `Detected ${browsers[availableBrowsers[0]].label}.`;
  } else {
    browserStatus.textContent = "Choose the browser where you loaded Voice Watch.";
  }
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
  if (navigator.brave?.isBrave && (await navigator.brave.isBrave())) {
    return "brave";
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
  unsupportedPage.hidden = false;
  browserField.hidden = true;
  browserHelpButton.disabled = true;
  registerButton.disabled = true;
}

function showSuccess(title, copy) {
  successTitle.textContent = title;
  successCopy.textContent = copy;
  setupFlow.hidden = true;
  unsupportedPage.hidden = true;
  successPage.hidden = false;
}

function renderRegistration() {
  const extensionId = sanitizeExtensionId(extensionIdInput.value);
  const browser = selectedBrowser();
  const hasValidId = extensionId.length > 0;
  const canRegister = hasValidId && Boolean(browser);

  idStatus.textContent = hasValidId
    ? "Extension ID looks valid."
    : "Paste the 32-character ID from the extension card.";
  idStatus.classList.toggle("valid", hasValidId);
  registerButton.disabled = !canRegister;

  registerLinkOutput.textContent = canRegister
    ? registrationLink(extensionId, browser)
    : "The registration link appears here after you paste a valid extension ID.";
}

function registrationLink(extensionId, browser) {
  return `voice-watch://register-native-host?extensionId=${extensionId}&browser=${encodeURIComponent(browser)}`;
}

function selectedBrowser() {
  return browserSelect.value && browsers[browserSelect.value] ? browserSelect.value : "";
}

function sanitizeExtensionId(value) {
  const trimmed = value.trim().toLowerCase();
  return /^[a-p]{32}$/.test(trimmed) ? trimmed : "";
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
