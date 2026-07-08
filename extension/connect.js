const connection = document.querySelector("#connection");
const result = document.querySelector("#result");
const connectButton = document.querySelector("#connect");
const checkButton = document.querySelector("#check");

connectButton.addEventListener("click", () => runAction("connect_native"));
checkButton.addEventListener("click", () => runAction("check_now"));

refreshStatus();

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

