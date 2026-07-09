const help = {
  chrome: {
    name: "Google Chrome",
    url: "chrome://extensions",
    steps: [
      "Open chrome://extensions in Chrome.",
      "Turn on Developer mode in the top-right corner.",
      "Use Load unpacked to select the Voice Watch extension folder."
    ]
  },
  edge: {
    name: "Microsoft Edge",
    url: "edge://extensions",
    steps: [
      "Open edge://extensions in Edge.",
      "Turn on Developer mode on the left side or top area.",
      "Use Load unpacked to select the Voice Watch extension folder."
    ]
  },
  brave: {
    name: "Brave",
    url: "brave://extensions",
    steps: [
      "Open brave://extensions in Brave.",
      "Turn on Developer mode in the top-right corner.",
      "Use Load unpacked to select the Voice Watch extension folder."
    ]
  },
  vivaldi: {
    name: "Vivaldi",
    url: "vivaldi://extensions",
    steps: [
      "Open vivaldi://extensions in Vivaldi.",
      "Turn on Developer mode.",
      "Use Load unpacked to select the Voice Watch extension folder."
    ]
  },
  opera: {
    name: "Opera",
    url: "opera://extensions",
    steps: [
      "Open opera://extensions in Opera.",
      "Turn on Developer mode.",
      "Use Load unpacked to select the Voice Watch extension folder."
    ]
  },
  chromium: {
    name: "Chromium-based browser",
    url: "chrome://extensions",
    steps: [
      "Open your browser's extensions page.",
      "Turn on Developer mode.",
      "Use Load unpacked to select the Voice Watch extension folder."
    ]
  }
};

const params = new URLSearchParams(window.location.search);
const browser = help[params.get("browser")] || help.chromium;

document.querySelector("#title").textContent = `${browser.name} setup help`;
document.querySelector("#summary").textContent = `Extensions page: ${browser.url}`;
const extensionsLink = document.querySelector("#extensions-link");
extensionsLink.href = browser.url;
extensionsLink.textContent = `Open ${browser.url}`;

const steps = document.querySelector("#steps");
browser.steps.forEach((step, index) => {
  const item = document.createElement("li");
  if (index === 0) {
    item.append("Open ");
    const link = document.createElement("a");
    link.href = browser.url;
    link.target = "_blank";
    link.rel = "noreferrer";
    link.textContent = browser.url;
    item.append(link, ` in ${browser.name}.`);
  } else {
    item.textContent = step;
  }
  steps.appendChild(item);
});

document.querySelector("#back").addEventListener("click", () => {
  window.location.href = "setup.html";
});
