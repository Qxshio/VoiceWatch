const assert = require("node:assert/strict");
const crypto = require("node:crypto");
const fs = require("node:fs");
const path = require("node:path");
const test = require("node:test");
const vm = require("node:vm");

const workerSource = fs.readFileSync(
  path.join(__dirname, "..", "extension", "service_worker.js"),
  "utf8"
);
const popupSource = fs.readFileSync(
  path.join(__dirname, "..", "extension", "connect.js"),
  "utf8"
);
const setupSource = fs.readFileSync(
  path.join(__dirname, "..", "extension", "setup.js"),
  "utf8"
);

test("the disconnected popup offers and performs an explicit reconnect", async () => {
  const popup = createPopupHarness();
  await popup.flush();

  assert.equal(popup.elements.get("#finish-setup").textContent, "Reconnect desktop");
  assert.equal(popup.elements.get("#finish-setup").hidden, false);

  await popup.click("#finish-setup");
  assert.equal(popup.messages.at(-1).type, "connect_native");
  assert.equal(popup.elements.get("#connection").textContent, "Desktop connected");
  assert.equal(popup.elements.get("#result").textContent, "Connected.");
  assert.equal(popup.elements.get("#finish-setup").disabled, false);
});

test("Firefox extension pages use the automatic finish flow", async () => {
  const setup = createFirefoxSetupHarness();
  await setup.flush();

  assert.equal(setup.elements.get("#extension-finish").hidden, false);
  assert.equal(setup.elements.get("#setup-flow").hidden, true);
  assert.match(setup.elements.get("#finish-status").textContent, /Firefox is ready/i);

  await setup.click("#finish-setup");
  assert.match(setup.window.location.href, /^voice-watch:\/\/register-native-host/);
  assert.match(setup.window.location.href, /browser=firefox/);
});

test("the setup page explains an extension version mismatch", async () => {
  const setup = createFirefoxSetupHarness(
    "?mode=update&desktopVersion=0.1.10&extensionVersion=0.1.9"
  );
  await setup.flush();

  assert.equal(setup.elements.get("#extension-update").hidden, false);
  assert.equal(setup.elements.get("#setup-flow").hidden, true);
  assert.match(setup.elements.get("#update-version").textContent, /0\.1\.10/);
  assert.match(setup.elements.get("#update-version").textContent, /0\.1\.9/);
});

test("a saved manual disconnect can reconnect without reinstalling", async () => {
  const storage = {};
  const firstWorker = createWorkerHarness(storage);

  await connectPort(firstWorker, 0);
  const disconnected = await firstWorker.sendMessage({ type: "disconnect_native" });
  assert.equal(disconnected.manuallyDisconnected, true);
  assert.equal(storage.manuallyDisconnected, true);

  const restartedWorker = createWorkerHarness(storage);
  const savedStatus = await restartedWorker.sendMessage({ type: "get_status" });
  assert.equal(savedStatus.nativeStatus.connected, false);
  assert.equal(savedStatus.manuallyDisconnected, true);
  assert.equal(restartedWorker.ports.length, 0);

  const reconnecting = restartedWorker.sendMessage({ type: "connect_native" });
  await restartedWorker.flush();
  assert.equal(restartedWorker.ports.length, 1);
  restartedWorker.ports[0].emitMessage(helloAck());

  const reconnected = await reconnecting;
  assert.equal(reconnected.nativeStatus.connected, true);
  assert.equal(reconnected.manuallyDisconnected, false);
  assert.equal(storage.manuallyDisconnected, false);
});

test("an old port disconnect cannot overwrite a replacement connection", async () => {
  const worker = createWorkerHarness({});
  await connectPort(worker, 0);

  await worker.sendMessage({ type: "disconnect_native" });
  const reconnecting = worker.sendMessage({ type: "connect_native" });
  await worker.flush();
  assert.equal(worker.ports.length, 2);
  worker.ports[1].emitMessage(helloAck());
  assert.equal((await reconnecting).nativeStatus.connected, true);

  await worker.runTimeouts(50);
  const current = await worker.sendMessage({ type: "get_status" });
  assert.equal(current.nativeStatus.connected, true);
  assert.equal(worker.ports.length, 2);
});

test("a timed-out native port is discarded before retrying", async () => {
  const worker = createWorkerHarness({});
  const firstAttempt = worker.sendMessage({ type: "get_status" });
  await worker.flush();
  assert.equal(worker.ports.length, 1);

  await worker.runTimeouts(2500);
  const timedOut = await firstAttempt;
  assert.equal(timedOut.nativeStatus.connected, false);
  assert.match(timedOut.nativeStatus.lastError, /timed out/i);

  const retry = worker.sendMessage({ type: "connect_native" });
  await worker.flush();
  assert.equal(worker.ports.length, 2);
  worker.ports[1].emitMessage(helloAck());
  assert.equal((await retry).nativeStatus.connected, true);
});

test("a native protocol error fails the connection immediately", async () => {
  const worker = createWorkerHarness({});
  const connecting = worker.sendMessage({ type: "get_status" });
  await worker.flush();

  worker.ports[0].emitMessage({
    type: "error",
    message: "Protocol version is unsupported."
  });
  const result = await connecting;

  assert.equal(result.nativeStatus.connected, false);
  assert.match(result.nativeStatus.lastError, /protocol version/i);
});

test("a desktop manual-check command returns a matching voice status", async () => {
  const worker = createWorkerHarness({});
  await connectPort(worker, 0);
  await worker.flush();
  const writesBeforeCheck = worker.storageWrites.length;

  worker.ports[0].emitMessage({
    type: "check_voice_status",
    requestId: "manual-check"
  });
  await worker.flush();

  const response = worker.ports[0].sentMessages.find(
    (message) => message.type === "voice_status" && message.requestId === "manual-check"
  );
  assert.ok(response, "manual check response was sent to the desktop host");
  assert.equal(response.ok, false);
  assert.equal(response.error.kind, "auth_error");
  assert.equal(worker.storageWrites.length, writesBeforeCheck + 1);
});

test("a desktop rejoin command opens the exact server in the connected browser", async () => {
  const worker = createWorkerHarness({});
  await connectPort(worker, 0);

  worker.ports[0].emitMessage({
    type: "rejoin",
    server: {
      placeId: 123,
      gameInstanceId: "1bb8dd1d-ad4c-43d2-a9c6-63feee836e43",
      detectedAtMs: 1000
    }
  });
  await worker.flush();

  assert.equal(worker.tabCreations.length, 1);
  assert.match(worker.tabCreations[0].url, /voiceWatchRejoin=1/);
  assert.match(
    worker.tabCreations[0].url,
    /gameInstanceId=1bb8dd1d-ad4c-43d2-a9c6-63feee836e43/
  );
});

test("a desktop extension-update command requests one browser update", async () => {
  const worker = createWorkerHarness({});
  await connectPort(worker, 0);

  worker.ports[0].emitMessage({
    type: "update_extension",
    desktopVersion: "0.1.10"
  });
  await worker.flush();
  await worker.flush();

  assert.equal(worker.extensionUpdateChecks, 1);
  assert.equal(worker.extensionReloads, 1);
});

test("polling fails closed when desktop readiness times out", async () => {
  const worker = createWorkerHarness({});
  await connectPort(worker, 0);

  await worker.runTimeouts(250);
  assert.equal(worker.ports[0].sentMessages.at(-1).type, "poll_readiness_request");

  await worker.runTimeouts(1500);
  assert.equal(worker.fetchCalls.length, 0);
});

test("presence checks run only while playing and are throttled", async () => {
  const worker = createWorkerHarness({});
  await connectPort(worker, 0);

  worker.ports[0].emitMessage(pollReadiness("waiting", false));
  await worker.flush();
  assert.equal(worker.fetchCalls.length, 0);

  worker.ports[0].emitMessage(pollReadiness("playing", true));
  await worker.flush();
  await worker.flush();
  assert.equal(worker.fetchCalls.length, 1);

  worker.ports[0].emitMessage(pollReadiness("still-playing", true));
  await worker.flush();
  assert.equal(worker.fetchCalls.length, 1);
});

test("unchanged readiness and server data do not repeat storage writes", async () => {
  const worker = createWorkerHarness({});
  await connectPort(worker, 0);

  const readiness = pollReadiness("first", false);
  const beforeReadiness = worker.storageWrites.length;
  worker.ports[0].emitMessage(readiness);
  await worker.flush();
  const afterFirstReadiness = worker.storageWrites.length;
  worker.ports[0].emitMessage({ ...readiness, requestId: "second" });
  await worker.flush();

  assert.equal(afterFirstReadiness, beforeReadiness + 1);
  assert.equal(worker.storageWrites.length, afterFirstReadiness);

  const server = {
    placeId: 123,
    gameInstanceId: "1bb8dd1d-ad4c-43d2-a9c6-63feee836e43",
    detectedAtMs: 1000
  };
  const beforeServer = worker.storageWrites.length;
  await worker.sendMessage({ type: "remember_last_server", server });
  await worker.sendMessage({
    type: "remember_last_server",
    server: { ...server, detectedAtMs: 2000 }
  });

  assert.equal(worker.storageWrites.length, beforeServer + 1);
  assert.equal(
    worker.ports[0].sentMessages.filter((message) => message.type === "last_server").length,
    1
  );
});

async function connectPort(worker, portIndex) {
  const connecting = worker.sendMessage({ type: "get_status" });
  await worker.flush();
  assert.equal(worker.ports.length, portIndex + 1);
  worker.ports[portIndex].emitMessage(helloAck());
  const response = await connecting;
  assert.equal(response.nativeStatus.connected, true);
}

function helloAck() {
  return {
    type: "hello_ack",
    appVersion: "test",
    protocolVersion: 1,
    pollIntervalSeconds: 10
  };
}

function pollReadiness(requestId, robloxPlaying) {
  return {
    type: "poll_readiness",
    requestId,
    pollIntervalSeconds: 10,
    shouldPoll: robloxPlaying,
    robloxRunning: robloxPlaying,
    robloxPlaying,
    microphoneActive: false,
    reason: robloxPlaying ? null : "roblox_not_playing",
    message: robloxPlaying ? null : "Waiting for a visible Roblox game window."
  };
}

function createWorkerHarness(storage) {
  let runtimeMessageListener = null;
  let nextTimerId = 1;
  const timers = new Map();
  const ports = [];
  const storageWrites = [];
  const fetchCalls = [];
  const tabCreations = [];
  let extensionUpdateChecks = 0;
  let extensionReloads = 0;

  const setTimer = (callback, delay, repeating) => {
    const id = nextTimerId++;
    timers.set(id, { callback, delay, repeating });
    return id;
  };

  const chrome = {
    storage: {
      local: {
        async get(keys) {
          return Object.fromEntries(
            keys
              .filter((key) => Object.hasOwn(storage, key))
              .map((key) => [key, clone(storage[key])])
          );
        },
        async set(values) {
          storageWrites.push(clone(values));
          Object.assign(storage, clone(values));
        }
      }
    },
    runtime: {
      lastError: null,
      getManifest: () => ({ version: "test" }),
      getURL: (value) => `chrome-extension://test/${value}`,
      async requestUpdateCheck() {
        extensionUpdateChecks += 1;
        return { status: "no_update" };
      },
      reload() {
        extensionReloads += 1;
      },
      connectNative() {
        const port = createPort();
        ports.push(port);
        return port;
      },
      onInstalled: { addListener() {} },
      onStartup: { addListener() {} },
      onUpdateAvailable: { addListener() {} },
      onMessage: {
        addListener(listener) {
          runtimeMessageListener = listener;
        }
      }
    },
    tabs: {
      async create(options) {
        tabCreations.push(clone(options));
      }
    }
  };

  const context = vm.createContext({
    chrome,
    console,
    crypto: { randomUUID: () => crypto.randomUUID() },
    URLSearchParams,
    fetch: async (...args) => {
      fetchCalls.push(clone(args));
      return {
        status: 401,
        ok: false,
        headers: { get: () => null },
        json: async () => ({})
      };
    },
    setTimeout: (callback, delay) => setTimer(callback, delay, false),
    clearTimeout: (id) => timers.delete(id),
    setInterval: (callback, delay) => setTimer(callback, delay, true),
    clearInterval: (id) => timers.delete(id)
  });
  new vm.Script(workerSource, { filename: "extension/service_worker.js" }).runInContext(context);

  return {
    ports,
    storageWrites,
    fetchCalls,
    tabCreations,
    get extensionUpdateChecks() {
      return extensionUpdateChecks;
    },
    get extensionReloads() {
      return extensionReloads;
    },
    async flush() {
      await new Promise((resolve) => setImmediate(resolve));
    },
    async runTimeouts(delay) {
      const matches = [...timers.entries()].filter(([, timer]) => timer.delay === delay);
      for (const [id, timer] of matches) {
        if (!timer.repeating) {
          timers.delete(id);
        }
        timer.callback();
        await this.flush();
      }
    },
    sendMessage(message) {
      assert.ok(runtimeMessageListener, "service worker message listener was registered");
      return new Promise((resolve) => {
        const keepAlive = runtimeMessageListener(message, {}, resolve);
        assert.equal(keepAlive, true);
      });
    }
  };
}

function createPopupHarness() {
  const selectors = [
    "#connection",
    "#result",
    "#desktop-status",
    "#desktop-detail",
    "#voice-status",
    "#voice-detail",
    "#last-checked",
    "#last-detail",
    "#finish-setup",
    "#disconnect"
  ];
  const elements = new Map(selectors.map((selector) => [selector, createElement()]));
  const messages = [];

  const disconnected = {
    ok: false,
    manuallyDisconnected: true,
    nativeStatus: {
      connected: false,
      connecting: false,
      lastError: "Disconnected."
    },
    lastVoiceStatus: null
  };
  const connected = {
    ok: true,
    manuallyDisconnected: false,
    nativeStatus: {
      connected: true,
      connecting: false,
      lastError: null,
      appVersion: "test",
      pollIntervalSeconds: 10,
      robloxLoggedIn: true
    },
    lastVoiceStatus: null
  };

  const context = vm.createContext({
    chrome: {
      runtime: {
        id: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        async sendMessage(message) {
          messages.push(clone(message));
          return message.type === "connect_native" ? connected : disconnected;
        }
      }
    },
    document: {
      querySelector(selector) {
        return elements.get(selector);
      }
    },
    window: {
      location: { href: "chrome-extension://test/connect.html" },
      setTimeout() {}
    },
    setInterval() {},
    Date,
    Number
  });
  new vm.Script(popupSource, { filename: "extension/connect.js" }).runInContext(context);

  return {
    elements,
    messages,
    async click(selector) {
      const listener = elements.get(selector).listeners.get("click");
      assert.ok(listener, `${selector} has a click listener`);
      await listener();
      await this.flush();
    },
    async flush() {
      await new Promise((resolve) => setImmediate(resolve));
    }
  };
}

function createFirefoxSetupHarness(search = "") {
  const selectors = [
    "#setup-flow",
    "#extension-update",
    "#success-page",
    "#unsupported-page",
    "#extension-finish",
    "#success-title",
    "#success-copy",
    "#page-title",
    "#page-copy",
    "#update-version",
    "#browser-field",
    "#browser-status",
    "#browser",
    "#path",
    "#finish-status",
    "#browser-help",
    "#finish-help",
    "#finish-setup",
    "#open-folder",
    "#copy-path",
    "#back-to-setup",
    "#update-help"
  ];
  const elements = new Map(selectors.map((selector) => [selector, createElement()]));
  const messages = [];
  const window = {
    location: {
      href: "moz-extension://test/setup.html",
      pathname: "/setup.html",
      protocol: "moz-extension:",
      search
    },
    setTimeout() {
      return 1;
    },
    clearTimeout() {}
  };
  const chrome = {
    runtime: {
      id: "voice-watch-connector@qxshio.github.io",
      async sendMessage(message) {
        messages.push(clone(message));
        return {
          ok: false,
          manuallyDisconnected: false,
          nativeStatus: { connected: false, lastError: "Native host is not registered." }
        };
      }
    }
  };
  const context = vm.createContext({
    chrome,
    console,
    decodeURIComponent,
    document: {
      querySelector(selector) {
        return elements.get(selector);
      },
      createElement() {
        return createElement();
      },
      execCommand() {
        return true;
      },
      body: createElement()
    },
    encodeURIComponent,
    navigator: { userAgent: "Mozilla/5.0 Firefox/142.0" },
    URLSearchParams,
    window
  });
  new vm.Script(setupSource, { filename: "extension/setup.js" }).runInContext(context);

  return {
    elements,
    messages,
    window,
    async click(selector) {
      const listener = elements.get(selector).listeners.get("click");
      assert.ok(listener, `${selector} has a click listener`);
      await listener();
      await this.flush();
    },
    async flush() {
      await new Promise((resolve) => setImmediate(resolve));
    }
  };
}

function createElement() {
  return {
    textContent: "",
    hidden: false,
    disabled: false,
    listeners: new Map(),
    children: [],
    addEventListener(type, listener) {
      this.listeners.set(type, listener);
    },
    appendChild(child) {
      this.children.push(child);
      return child;
    },
    replaceChildren(...children) {
      this.children = children;
    },
    setAttribute() {},
    select() {},
    remove() {
      this.removed = true;
    }
  };
}

function createPort() {
  const messageListeners = [];
  const disconnectListeners = [];
  let closed = false;

  return {
    sentMessages: [],
    onMessage: {
      addListener(listener) {
        messageListeners.push(listener);
      }
    },
    onDisconnect: {
      addListener(listener) {
        disconnectListeners.push(listener);
      }
    },
    postMessage(message) {
      if (closed) {
        throw new Error("Port is closed.");
      }
      this.sentMessages.push(clone(message));
    },
    disconnect() {
      if (closed) {
        return;
      }
      closed = true;
      for (const listener of disconnectListeners) {
        listener();
      }
    },
    emitMessage(message) {
      for (const listener of messageListeners) {
        listener(clone(message));
      }
    }
  };
}

function clone(value) {
  return JSON.parse(JSON.stringify(value));
}
