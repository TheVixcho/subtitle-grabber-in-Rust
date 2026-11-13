const { invoke } = window.__TAURI__.core;

const STORAGE_DEST_KEY = "subgrabber_default_dir";
const STORAGE_THEME_KEY = "subgrabber_theme";
const STORAGE_CSS_KEY = "subgrabber_custom_css";
const STORAGE_PALETTE_KEY = "subgrabber_palette_colors";

const PALETTE_DEFAULTS = {
  obsidian: { bg: "#0d0f14", accent: "#38bdf8" },
  spotify: { bg: "#121212", accent: "#1db954" },
  nord: { bg: "#2e3440", accent: "#88c0d0" },
  dracula: { bg: "#21222c", accent: "#bd93f9" },
  monokai: { bg: "#1e1f1c", accent: "#a6e22e" },
  light: { bg: "#f6f8fa", accent: "#0969da" },
};

function clearInlinePalette() {
  const rootStyle = document.documentElement.style;
  rootStyle.removeProperty("--bg-root");
  rootStyle.removeProperty("--accent-cyan");
  rootStyle.removeProperty("--btn-primary-bg");
}

function applyInlinePalette(bg, accent) {
  const rootStyle = document.documentElement.style;
  if (bg) {
    rootStyle.setProperty("--bg-root", bg);
    $("picker-bg").value = bg;
  }
  if (accent) {
    rootStyle.setProperty("--accent-cyan", accent);
    rootStyle.setProperty("--btn-primary-bg", accent);
    $("picker-accent").value = accent;
  }
}

function applyTheme(themeName, keepCustomColors = false) {
  const theme = PALETTE_DEFAULTS[themeName] ? themeName : "obsidian";
  if (theme === "obsidian") {
    document.documentElement.removeAttribute("data-theme");
  } else {
    document.documentElement.setAttribute("data-theme", theme);
  }
  localStorage.setItem(STORAGE_THEME_KEY, theme);
  document.querySelectorAll(".color-tile").forEach((tile) => {
    tile.classList.toggle("active", tile.dataset.theme === theme);
  });

  if (!keepCustomColors) {
    clearInlinePalette();
    localStorage.removeItem(STORAGE_PALETTE_KEY);
    const defs = PALETTE_DEFAULTS[theme];
    $("picker-bg").value = defs.bg;
    $("picker-accent").value = defs.accent;
  }
}

function applyCustomCss(cssText) {
  const css = cssText || "";
  $("custom-theme-style").textContent = css;
  $("custom-css-input").value = css;
  if (css.trim()) {
    localStorage.setItem(STORAGE_CSS_KEY, css);
  } else {
    localStorage.removeItem(STORAGE_CSS_KEY);
  }
}

const state = {
  appInfo: null,
  ytdlp: null,
  video: null,
  selectedTrack: null,
  filterKind: "all",
  filterQuery: "",
  destDir: "",
  lastDownload: null,
  busy: false,
};

const $ = (id) => document.getElementById(id);

function setStatus(text) {
  $("status-message").textContent = text;
}

function appendLog(message, level = "info") {
  const container = $("console-log");
  const line = document.createElement("div");
  line.className = "log-line";

  const time = document.createElement("span");
  time.className = "log-time";
  time.textContent = new Date().toLocaleTimeString([], {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  });

  const msg = document.createElement("span");
  msg.className = `log-${level}`;
  msg.textContent = message;

  line.appendChild(time);
  line.appendChild(msg);
  container.appendChild(line);
  container.scrollTop = container.scrollHeight;
}

function formatBytes(bytes) {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(2)} MB`;
}

function switchView(viewName) {
  document.querySelectorAll(".nav-item").forEach((btn) => {
    btn.classList.toggle("active", btn.dataset.view === viewName);
  });
  document.querySelectorAll(".view").forEach((sec) => {
    sec.classList.toggle("active", sec.id === `view-${viewName}`);
  });
  if (viewName === "library") {
    refreshLibrary();
  }
}

function showSplash(autoHide = true) {
  const splash = $("splash");
  splash.classList.remove("fade-out");
  let timer = null;

  const dismiss = () => {
    if (timer) clearTimeout(timer);
    splash.classList.add("fade-out");
    splash.removeEventListener("click", dismiss);
    window.removeEventListener("keydown", dismiss);
  };

  splash.addEventListener("click", dismiss);
  window.addEventListener("keydown", dismiss);
  if (autoHide) {
    timer = setTimeout(dismiss, 1150);
  }
}

function updateDestDisplays(dir) {
  state.destDir = dir;
  $("dest-input").value = dir;
  $("settings-default-dir").value = dir;
  $("status-dest").textContent = dir;
}

function updateYtdlpUI(status) {
  state.ytdlp = status;
  const available = Boolean(status && status.available);

  $("sidebar-engine-dot").className = `dot ${available ? "dot-ok" : "dot-warn"}`;
  $("status-dot").className = `dot ${available ? "dot-ok" : "dot-warn"}`;

  if (available) {
    $("sidebar-engine-label").textContent = `yt-dlp v${status.version}`;
    $("sidebar-install-btn").classList.add("hidden");
    $("ytdlp-alert").classList.add("hidden");
    $("status-engine-text").textContent = `yt-dlp v${status.version}`;

    $("settings-engine-status").textContent = "Ready";
    $("settings-engine-version").textContent = status.version || "—";
    $("settings-engine-path").textContent = status.path || "—";
  } else {
    $("sidebar-engine-label").textContent = "yt-dlp missing";
    $("sidebar-install-btn").classList.remove("hidden");
    $("ytdlp-alert").classList.remove("hidden");
    $("status-engine-text").textContent = "yt-dlp missing";

    $("settings-engine-status").textContent = "Not installed";
    $("settings-engine-version").textContent = "—";
    $("settings-engine-path").textContent = state.appInfo?.managed_ytdlp_path || "—";
  }
}

async function refreshEngineStatus() {
  try {
    const status = await invoke("check_ytdlp");
    updateYtdlpUI(status);
    return status;
  } catch (err) {
    appendLog(`Engine check failed: ${err}`, "err");
    return null;
  }
}

async function installYtdlp() {
  if (state.busy) return;
  state.busy = true;
  $("install-spinner").classList.remove("hidden");
  $("alert-install-text").textContent = "Installing...";
  $("alert-install-btn").disabled = true;
  $("sidebar-install-btn").disabled = true;
  $("settings-install-btn").disabled = true;
  setStatus("Downloading yt-dlp...");
  appendLog("Downloading yt-dlp...", "info");

  try {
    const status = await invoke("install_ytdlp");
    updateYtdlpUI(status);
    setStatus(`yt-dlp v${status.version} ready`);
    appendLog(`Installed yt-dlp v${status.version}`, "ok");
  } catch (err) {
    setStatus("Install failed");
    appendLog(String(err), "err");
  } finally {
    state.busy = false;
    $("install-spinner").classList.add("hidden");
    $("alert-install-text").textContent = "Install yt-dlp";
    $("alert-install-btn").disabled = false;
    $("sidebar-install-btn").disabled = false;
    $("settings-install-btn").disabled = false;
  }
}

function renderTracks(animate = false) {
  const list = $("tracks-list");
  list.innerHTML = "";

  if (!state.video) return;

  const query = state.filterQuery.trim().toLowerCase();
  const filtered = state.video.languages.filter((track) => {
    if (state.filterKind === "official" && track.auto) return false;
    if (state.filterKind === "auto" && !track.auto) return false;
    if (query) {
      const hay = `${track.code} ${track.name}`.toLowerCase();
      if (!hay.includes(query)) return false;
    }
    return true;
  });

  if (filtered.length === 0) {
    const empty = document.createElement("div");
    empty.className = "empty-placeholder";
    empty.textContent = "No matching subtitles.";
    list.appendChild(empty);
    return;
  }

  filtered.forEach((track, idx) => {
    const row = document.createElement("div");
    const isSelected =
      state.selectedTrack &&
      state.selectedTrack.code === track.code &&
      state.selectedTrack.auto === track.auto;
    row.className = `track-row${isSelected ? " selected" : ""}`;
    if (animate) {
      row.style.animationDelay = `${Math.min(idx * 18, 220)}ms`;
    } else {
      row.style.animation = "none";
    }

    const code = document.createElement("span");
    code.className = "track-code";
    code.textContent = track.code;

    const name = document.createElement("span");
    name.className = "track-name";
    name.textContent = track.name;

    const tag = document.createElement("span");
    tag.className = `tag ${track.auto ? "tag-auto" : "tag-official"}`;
    tag.textContent = track.auto ? "Auto" : "Official";

    row.appendChild(code);
    row.appendChild(name);
    row.appendChild(tag);

    row.addEventListener("click", () => selectTrack(track));
    row.addEventListener("dblclick", () => {
      selectTrack(track);
      downloadSelected();
    });

    list.appendChild(row);
  });
}

function selectTrack(track, animate = false) {
  state.selectedTrack = track;
  if (track) {
    const kind = track.auto ? "auto" : "official";
    $("selected-track-display").textContent = `${track.code} — ${track.name} (${kind})`;
    $("download-btn").disabled = false;
  } else {
    $("selected-track-display").textContent = "Select a track";
    $("download-btn").disabled = true;
  }
  renderTracks(animate);
}

async function fetchSubtitles() {
  if (state.busy) return;
  const url = $("url-input").value.trim();
  if (!url) {
    setStatus("Enter a YouTube URL");
    $("url-input").focus();
    return;
  }

  state.busy = true;
  $("fetch-spinner").classList.remove("hidden");
  $("fetch-btn-text").textContent = "Fetching...";
  $("fetch-btn").disabled = true;
  $("download-result-card").classList.add("hidden");
  setStatus("Looking up subtitles...");
  appendLog(`Fetching ${url}`, "info");

  try {
    const info = await invoke("fetch_subtitles", { url });
    state.video = info;
    state.lastDownload = null;

    $("welcome-state").classList.add("hidden");
    $("workbench").classList.remove("hidden");

    $("video-title").textContent = info.title;
    $("video-channel").textContent = info.channel || "YouTube";
    $("video-duration").textContent = info.duration || "—";
    $("video-count").textContent = `${info.languages.length} track${info.languages.length === 1 ? "" : "s"}`;

    if (info.thumbnail) {
      $("video-thumb").src = info.thumbnail;
    } else {
      $("video-thumb").removeAttribute("src");
    }

    if (info.languages.length === 0) {
      selectTrack(null, true);
      appendLog(`"${info.title}" has no subtitles available.`, "warn");
      setStatus("No subtitles found");
    } else {
      const preferred =
        info.languages.find((t) => !t.auto && t.code.startsWith("en")) ||
        info.languages.find((t) => !t.auto) ||
        info.languages[0];
      selectTrack(preferred, true);
      appendLog(`Found ${info.languages.length} tracks.`, "ok");
      setStatus(`Loaded ${info.languages.length} tracks`);
    }
  } catch (err) {
    appendLog(String(err), "err");
    setStatus(String(err));
    if (!state.video) {
      $("welcome-state").classList.add("hidden");
      $("workbench").classList.remove("hidden");
      $("video-title").textContent = "Lookup Error";
      $("video-channel").textContent = "—";
      $("video-duration").textContent = "—";
      $("video-count").textContent = "0 tracks";
      $("tracks-list").innerHTML = `<div class="empty-placeholder">${String(err)}</div>`;
      selectTrack(null);
    }
  } finally {
    state.busy = false;
    $("fetch-spinner").classList.add("hidden");
    $("fetch-btn-text").textContent = "Fetch";
    $("fetch-btn").disabled = false;
  }
}

async function downloadSelected() {
  if (state.busy || !state.selectedTrack) return;
  const url = $("url-input").value.trim();
  const outDir = $("dest-input").value.trim() || state.destDir;
  updateDestDisplays(outDir);

  state.busy = true;
  $("download-spinner").classList.remove("hidden");
  $("download-btn-text").textContent = "Downloading...";
  $("download-btn").disabled = true;
  $("download-result-card").classList.add("hidden");
  setStatus(`Downloading [${state.selectedTrack.code}]...`);
  appendLog(`Downloading ${state.selectedTrack.code}...`, "info");

  try {
    const res = await invoke("download_subtitle_cmd", {
      url,
      langCode: state.selectedTrack.code,
      auto: state.selectedTrack.auto,
      outDir,
    });
    state.lastDownload = res;
    $("result-filename").textContent = `${res.filename} (${formatBytes(res.size_bytes)})`;
    $("download-result-card").classList.remove("hidden");
    appendLog(`Saved: ${res.filename}`, "ok");
    setStatus(`Saved ${res.filename}`);
    refreshLibrary();
  } catch (err) {
    appendLog(String(err), "err");
    setStatus("Download failed");
  } finally {
    state.busy = false;
    $("download-spinner").classList.add("hidden");
    $("download-btn-text").textContent = "Download .SRT";
    $("download-btn").disabled = !state.selectedTrack;
  }
}

async function browseFolder(targetInputId) {
  const current = $(targetInputId).value.trim() || state.destDir;
  const picked = await invoke("pick_folder", { initialDir: current });
  if (picked) {
    updateDestDisplays(picked);
    if (targetInputId === "settings-default-dir") {
      localStorage.setItem(STORAGE_DEST_KEY, picked);
    }
    refreshLibrary();
  }
}

async function refreshLibrary() {
  const dir = $("dest-input").value.trim() || state.destDir;
  try {
    const items = await invoke("list_saved_subtitles", { dir });
    $("library-count").textContent = String(items.length);

    const list = $("library-list");
    list.innerHTML = "";
    if (items.length === 0) {
      const empty = document.createElement("div");
      empty.className = "empty-placeholder";
      empty.textContent = "No .srt files yet.";
      list.appendChild(empty);
      return;
    }

    for (const item of items) {
      const el = document.createElement("div");
      el.className = "library-item";

      const name = document.createElement("div");
      name.className = "library-name";
      name.textContent = item.filename;

      const meta = document.createElement("div");
      meta.className = "library-meta";
      const date = item.modified_epoch_secs
        ? new Date(item.modified_epoch_secs * 1000).toLocaleString()
        : "";
      meta.textContent = `${formatBytes(item.size_bytes)}${date ? " • " + date : ""}`;

      el.appendChild(name);
      el.appendChild(meta);

      el.addEventListener("click", async () => {
        document.querySelectorAll(".library-item").forEach((n) => n.classList.remove("selected"));
        el.classList.add("selected");
        $("preview-title").textContent = item.filename;
        try {
          const text = await invoke("read_subtitle_preview", { path: item.path });
          $("preview-content").textContent = text;
        } catch (err) {
          $("preview-content").textContent = String(err);
        }
      });

      list.appendChild(el);
    }
  } catch {
    $("library-count").textContent = "0";
  }
}

function bindEvents() {
  document.querySelectorAll(".nav-item").forEach((btn) => {
    btn.addEventListener("click", () => switchView(btn.dataset.view));
  });

  $("status-engine-btn").addEventListener("click", () => switchView("engine"));

  const urlInput = $("url-input");
  urlInput.addEventListener("input", () => {
    $("clear-url-btn").classList.toggle("hidden", !urlInput.value);
  });
  urlInput.addEventListener("keydown", (e) => {
    if (e.key === "Enter") fetchSubtitles();
  });

  $("clear-url-btn").addEventListener("click", () => {
    urlInput.value = "";
    $("clear-url-btn").classList.add("hidden");
    urlInput.focus();
  });

  $("paste-btn").addEventListener("click", async () => {
    try {
      const text = await navigator.clipboard.readText();
      if (text) {
        urlInput.value = text.trim();
        $("clear-url-btn").classList.remove("hidden");
        urlInput.focus();
      }
    } catch {
      urlInput.focus();
      setStatus("Use Ctrl+V to paste");
    }
  });

  $("fetch-btn").addEventListener("click", fetchSubtitles);
  $("download-btn").addEventListener("click", downloadSelected);

  document.querySelectorAll(".filter-tab").forEach((tab) => {
    tab.addEventListener("click", () => {
      document.querySelectorAll(".filter-tab").forEach((t) => t.classList.remove("active"));
      tab.classList.add("active");
      state.filterKind = tab.dataset.filter;
      renderTracks();
    });
  });

  $("track-filter").addEventListener("input", (e) => {
    state.filterQuery = e.target.value;
    renderTracks();
  });

  $("browse-btn").addEventListener("click", () => browseFolder("dest-input"));
  $("open-dest-btn").addEventListener("click", () => {
    invoke("open_in_file_manager", { path: $("dest-input").value.trim() || state.destDir });
  });
  $("result-open-btn").addEventListener("click", () => {
    const target = state.lastDownload?.path || $("dest-input").value.trim() || state.destDir;
    invoke("open_in_file_manager", { path: target });
  });
  $("result-preview-btn").addEventListener("click", () => {
    if (!state.lastDownload) return;
    switchView("library");
    $("preview-title").textContent = state.lastDownload.filename;
    $("preview-content").textContent = state.lastDownload.preview || "(empty)";
  });

  $("library-refresh-btn").addEventListener("click", refreshLibrary);
  $("library-open-btn").addEventListener("click", () => {
    invoke("open_in_file_manager", { path: $("dest-input").value.trim() || state.destDir });
  });

  $("sidebar-install-btn").addEventListener("click", installYtdlp);
  $("alert-install-btn").addEventListener("click", installYtdlp);
  $("settings-install-btn").addEventListener("click", installYtdlp);
  $("settings-recheck-btn").addEventListener("click", refreshEngineStatus);

  $("settings-browse-btn").addEventListener("click", () => browseFolder("settings-default-dir"));
  $("settings-save-dir-btn").addEventListener("click", () => {
    const val = $("settings-default-dir").value.trim();
    if (val) {
      localStorage.setItem(STORAGE_DEST_KEY, val);
      updateDestDisplays(val);
      setStatus("Default folder saved");
      refreshLibrary();
    }
  });
  $("settings-reset-dir-btn").addEventListener("click", () => {
    localStorage.removeItem(STORAGE_DEST_KEY);
    if (state.appInfo?.default_dir) {
      updateDestDisplays(state.appInfo.default_dir);
      setStatus("Reset to default");
      refreshLibrary();
    }
  });

  $("replay-splash-btn").addEventListener("click", () => showSplash(true));

  document.querySelectorAll(".color-tile").forEach((tile) => {
    tile.addEventListener("click", () => {
      applyTheme(tile.dataset.theme, false);
      setStatus("Colors updated");
    });
  });

  const savePickedColors = () => {
    const bg = $("picker-bg").value;
    const accent = $("picker-accent").value;
    applyInlinePalette(bg, accent);
    localStorage.setItem(STORAGE_PALETTE_KEY, JSON.stringify({ bg, accent }));
    setStatus("Colors updated");
  };

  $("picker-accent").addEventListener("input", savePickedColors);
  $("picker-bg").addEventListener("input", savePickedColors);

  $("reset-colors-btn").addEventListener("click", () => {
    applyTheme("obsidian", false);
    setStatus("Colors reset");
  });

  $("apply-css-btn").addEventListener("click", () => {
    applyCustomCss($("custom-css-input").value);
    setStatus("Custom CSS applied");
  });

  $("import-css-btn").addEventListener("click", async () => {
    try {
      const content = await invoke("pick_css_file");
      if (content !== null && content !== undefined) {
        applyCustomCss(content);
        setStatus("Imported custom CSS");
      }
    } catch (err) {
      setStatus(String(err));
    }
  });

  $("clear-css-btn").addEventListener("click", () => {
    applyCustomCss("");
    setStatus("Custom CSS cleared");
  });
}

async function init() {
  const savedTheme = localStorage.getItem(STORAGE_THEME_KEY) || "obsidian";
  let savedPalette = null;
  try {
    savedPalette = JSON.parse(localStorage.getItem(STORAGE_PALETTE_KEY) || "null");
  } catch {
    savedPalette = null;
  }
  applyTheme(savedTheme, Boolean(savedPalette));
  if (savedPalette) {
    applyInlinePalette(savedPalette.bg, savedPalette.accent);
  }
  applyCustomCss(localStorage.getItem(STORAGE_CSS_KEY) || "");
  bindEvents();

  try {
    const info = await invoke("get_app_info");
    state.appInfo = info;

    $("splash-ascii").textContent = info.ascii_art;
    $("welcome-ascii").textContent = info.ascii_art;
    $("settings-ascii").textContent = info.ascii_art;

    const savedDir = localStorage.getItem(STORAGE_DEST_KEY);
    updateDestDisplays(savedDir || info.default_dir);
  } catch {
    setStatus("Failed to load app info");
  }

  showSplash(true);
  await refreshEngineStatus();
  await refreshLibrary();
}

window.addEventListener("DOMContentLoaded", init);
