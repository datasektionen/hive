const persistedTheme = localStorage.getItem("theme");

if (persistedTheme === "dark" || persistedTheme === "light") {
  document.documentElement.dataset.theme = persistedTheme;
}

function getThemePref() {
  if (
    window.matchMedia &&
    window.matchMedia("(prefers-color-scheme: dark").matches
  ) {
    return "dark";
  } else {
    return "light";
  }
}

function toggleTheme() {
  const current = document.documentElement.dataset.theme ?? getThemePref();
  const other = current === "dark" ? "light" : "dark";

  document.documentElement.dataset.theme = other;
  localStorage.setItem("theme", other);
}

function switchLang(target) {
  document.cookie = `Hive-Lang=${target}; Secure`;
  window.location.reload();
}
