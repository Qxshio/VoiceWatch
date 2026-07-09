const pathOutput = document.querySelector("#path");
const commandOutput = document.querySelector("#command");
const extensionIdInput = document.querySelector("#extension-id");
const idStatus = document.querySelector("#id-status");
const openChromeButton = document.querySelector("#open-chrome");
const openEdgeButton = document.querySelector("#open-edge");
const openFolderButton = document.querySelector("#open-folder");
const copyPathButton = document.querySelector("#copy-path");
const copyCommandButton = document.querySelector("#copy-command");

const folderPath = decodeURIComponent(
  window.location.pathname.replace(/^\/([A-Za-z]:)/, "$1")
)
  .replace(/\/setup\.html$/i, "")
  .replace(/\//g, "\\");
const appRoot = folderPath.replace(/\\extension$/i, "");

pathOutput.textContent = folderPath;
renderCommand();

extensionIdInput.addEventListener("input", renderCommand);

openChromeButton.addEventListener("click", () => {
  window.location.href = "chrome://extensions";
});

openEdgeButton.addEventListener("click", () => {
  window.location.href = "edge://extensions";
});

openFolderButton.addEventListener("click", () => {
  window.location.href = "./";
});

copyPathButton.addEventListener("click", async () => {
  await copyText(folderPath);
  pathOutput.textContent = `Copied folder path:\n${folderPath}`;
});

copyCommandButton.addEventListener("click", async () => {
  const command = registrationCommand();
  await copyText(command);
  commandOutput.textContent = `Copied command:\n${command}`;
});

function renderCommand() {
  const extensionId = sanitizeExtensionId(extensionIdInput.value);
  const hasValidId = extensionId.length > 0;

  extensionIdInput.value = extensionIdInput.value.trim();
  idStatus.textContent = hasValidId
    ? "Extension ID looks valid."
    : "Paste the 32-character ID from the extension card.";
  idStatus.classList.toggle("valid", hasValidId);
  copyCommandButton.disabled = !hasValidId;
  commandOutput.textContent = registrationCommand();
}

function registrationCommand() {
  const extensionId = sanitizeExtensionId(extensionIdInput.value) || "<paste-extension-id>";
  const scriptPath = `${appRoot}\\scripts\\register-native-host.ps1`;
  return `powershell.exe -NoProfile -ExecutionPolicy Bypass -File "${scriptPath}" -ExtensionId "${extensionId}" -Browser Both`;
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
