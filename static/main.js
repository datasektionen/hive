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

function openModal(id) {
  const modal = document.getElementById(id);
  const html = document.documentElement;
  const scrollbarWidth = window.innerWidth - html.clientWidth;
  if (scrollbarWidth) {
    html.style.setProperty("--pico-scrollbar-width", `${scrollbarWidth}px`);
  }
  html.classList.add("modal-is-open", "modal-is-opening");
  setTimeout(() => html.classList.remove("modal-is-opening"), 300);
  modal.showModal();
}

function closeModal(id) {
  const modal = document.getElementById(id);
  const html = document.documentElement;
  html.classList.add("modal-is-closing");
  setTimeout(() => {
    html.classList.remove("modal-is-closing", "modal-is-open");
    html.style.removeProperty("--pico-scrollbar-width");
    modal.close();
  }, 300);
}

// these 2 handlers make hx-indicator automatically work with Pico loading
document.body.addEventListener("htmx:beforeSend", () => {
  document
    .querySelectorAll(".htmx-request")
    .forEach((el) => el.setAttribute("aria-busy", "true"));
});
document.body.addEventListener("htmx:beforeOnLoad", () => {
  document
    .querySelectorAll(".htmx-request")
    .forEach((el) => el.removeAttribute("aria-busy"));
});
