const pathOutput = document.querySelector("#path");
const openFolderButton = document.querySelector("#open-folder");
const copyPathButton = document.querySelector("#copy-path");

const folderPath = decodeURIComponent(
  window.location.pathname.replace(/^\/([A-Za-z]:)/, "$1")
)
  .replace(/\/setup\.html$/i, "")
  .replace(/\//g, "\\");

pathOutput.textContent = folderPath;

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
