const pathOutput = document.querySelector("#path");
const commandOutput = document.querySelector("#command");
const extensionIdInput = document.querySelector("#extension-id");
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

openFolderButton.addEventListener("click", () => {
  window.location.href = "./";
});

copyPathButton.addEventListener("click", async () => {
  try {
    await navigator.clipboard.writeText(folderPath);
    pathOutput.textContent = `Copied:\n${folderPath}`;
  } catch (_error) {
    pathOutput.textContent = folderPath;
  }
});

copyCommandButton.addEventListener("click", async () => {
  const command = registrationCommand();
  try {
    await navigator.clipboard.writeText(command);
    commandOutput.textContent = `Copied:\n${command}`;
  } catch (_error) {
    commandOutput.textContent = command;
  }
});

function renderCommand() {
  commandOutput.textContent = registrationCommand();
}

function registrationCommand() {
  const extensionId = sanitizeExtensionId(extensionIdInput.value) || "<extension-id>";
  const scriptPath = `${appRoot}\\scripts\\register-native-host.ps1`;
  return `powershell.exe -NoProfile -ExecutionPolicy Bypass -File "${scriptPath}" -ExtensionId "${extensionId}" -Browser Both`;
}

function sanitizeExtensionId(value) {
  const trimmed = value.trim().toLowerCase();
  return /^[a-p]{32}$/.test(trimmed) ? trimmed : "";
}
