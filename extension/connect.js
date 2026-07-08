const connection = document.querySelector("#connection");
const result = document.querySelector("#result");
const connectButton = document.querySelector("#connect");
const checkButton = document.querySelector("#check");

const hasExtensionRuntime =
  typeof chrome !== "undefined" && chrome.runtime?.sendMessage;

if (hasExtensionRuntime) {
  connectButton.addEventListener("click", () => runAction("connect_native"));
  checkButton.addEventListener("click", () => runAction("check_now"));
  refreshStatus();
} else {
  connection.textContent = "Open from the browser extension";
  result.textContent =
    "This page only works after Voice Watch is loaded as a Chrome or Edge extension. Open setup.html from the bundled extension folder for install steps.";
  connectButton.disabled = true;
  checkButton.disabled = true;
}

async function refreshStatus() {
  const response = await chrome.runtime.sendMessage({ type: "get_status" });
  renderStatus(response?.nativeStatus);
}

async function runAction(type) {
  setBusy(true);
  try {
    const response = await chrome.runtime.sendMessage({ type });
    result.textContent = JSON.stringify(response, null, 2);
    renderStatus(response?.nativeStatus);
  } catch (error) {
    result.textContent = error.message || String(error);
  } finally {
    setBusy(false);
  }
}

function renderStatus(status) {
  if (!status) {
    connection.textContent = "Not connected";
    return;
  }

  if (status.connected) {
    const version = status.appVersion ? `Desktop ${status.appVersion}` : "Desktop connected";
    connection.textContent = version;
  } else {
    connection.textContent = status.lastError || "Not connected";
  }
}

function setBusy(isBusy) {
  connectButton.disabled = isBusy;
  checkButton.disabled = isBusy;
}
