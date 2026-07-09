const browserSelect = document.querySelector("#browser");
const pathOutput = document.querySelector("#path");
const registerLinkOutput = document.querySelector("#register-link");
const extensionIdInput = document.querySelector("#extension-id");
const idStatus = document.querySelector("#id-status");
const openBrowserButton = document.querySelector("#open-browser");
const browserHelpButton = document.querySelector("#browser-help");
const openFolderButton = document.querySelector("#open-folder");
const copyPathButton = document.querySelector("#copy-path");
const registerButton = document.querySelector("#register");

const extensionPages = {
  all: "chrome://extensions",
  chrome: "chrome://extensions",
  edge: "edge://extensions",
  brave: "brave://extensions",
  vivaldi: "vivaldi://extensions",
  opera: "opera://extensions",
  chromium: "chrome://extensions"
};

const helpTargets = new Map([
  ["chrome", "chrome"],
  ["google chrome", "chrome"],
  ["edge", "edge"],
  ["microsoft edge", "edge"],
  ["brave", "brave"],
  ["vivaldi", "vivaldi"],
  ["opera", "opera"],
  ["chromium", "chromium"]
]);

const folderPath = decodeURIComponent(
  window.location.pathname.replace(/^\/([A-Za-z]:)/, "$1")
)
  .replace(/\/setup\.html$/i, "")
  .replace(/\//g, "\\");

pathOutput.textContent = folderPath;
renderRegistration();

browserSelect.addEventListener("change", renderRegistration);
extensionIdInput.addEventListener("input", renderRegistration);

openBrowserButton.addEventListener("click", () => {
  const browser = browserSelect.value === "all" ? askForBrowser() : browserSelect.value;
  if (!browser) {
    return;
  }

  window.location.href = extensionPages[browser] || extensionPages.chromium;
});

browserHelpButton.addEventListener("click", () => {
  const browser = askForBrowser();
  if (!browser) {
    return;
  }

  window.location.href = `help.html?browser=${encodeURIComponent(browser)}`;
});

function askForBrowser() {
  const answer = window.prompt(
    "Which browser are you using? Examples: Chrome, Edge, Brave, Vivaldi, Opera, Chromium"
  );
  if (!answer) {
    return null;
  }

  return helpTargets.get(answer.trim().toLowerCase()) || "chromium";
}

openFolderButton.addEventListener("click", () => {
  window.location.href = "./";
});

copyPathButton.addEventListener("click", async () => {
  await copyText(folderPath);
  pathOutput.textContent = `Copied folder path:\n${folderPath}`;
});

registerButton.addEventListener("click", () => {
  const extensionId = sanitizeExtensionId(extensionIdInput.value);
  if (!extensionId) {
    renderRegistration();
    return;
  }

  const link = registrationLink(extensionId);
  registerLinkOutput.textContent = `Opening Voice Watch setup:\n${link}`;
  window.location.href = link;
});

function renderRegistration() {
  const extensionId = sanitizeExtensionId(extensionIdInput.value);
  const hasValidId = extensionId.length > 0;

  idStatus.textContent = hasValidId
    ? "Extension ID looks valid."
    : "Paste the 32-character ID from the extension card.";
  idStatus.classList.toggle("valid", hasValidId);
  registerButton.disabled = !hasValidId;

  registerLinkOutput.textContent = hasValidId
    ? registrationLink(extensionId)
    : "The registration link appears here after you paste a valid extension ID.";
}

function registrationLink(extensionId) {
  const browser = encodeURIComponent(browserSelect.value);
  return `voice-watch://register-native-host?extensionId=${extensionId}&browser=${browser}`;
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
