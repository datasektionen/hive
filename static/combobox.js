for (const ul of document.querySelectorAll(".combobox ul")) {
  const container = ul.parentElement;
  const input = container.querySelector("input");

  container.addEventListener("focusin", () =>
    ul.classList.add("combobox-open")
  );

  container.addEventListener("focusout", () => {
    ul.classList.remove("combobox-open");

    for (const li of ul.querySelectorAll("li[data-combobox-value]")) {
      if (input.value == li.dataset.comboboxValue) {
        // matched

        if (li.dataset.comboboxEnable) {
          const other = document.getElementById(li.dataset.comboboxEnable);
          other.toggleAttribute("required", true);
          other.toggleAttribute("disabled", false);
        }
        if (li.dataset.comboboxDisable) {
          const other = document.getElementById(li.dataset.comboboxDisable);
          other.toggleAttribute("required", false);
          other.toggleAttribute("disabled", true);
          other.value = "";
        }

        return;
      }
    }

    // not a valid value; reset
    input.value = "";
  });

  const selectOption = (event) => {
    const li = event.target.closest("li[data-combobox-value]");
    input.value = li.dataset.comboboxValue;
    ul.classList.remove("combobox-open");
  };

  const filterOptions = () => {
    const query = input.value.toLowerCase();

    for (const li of ul.querySelectorAll("li[data-combobox-value]")) {
      if (li.textContent.toLowerCase().includes(query)) {
        li.classList.add("combobox-match");

        // only actually added if not already present with same callback
        li.addEventListener("click", selectOption);
      } else {
        li.classList.remove("combobox-match");
      }
    }

    ul.classList.add("combobox-open");
  };

  input.addEventListener("input", filterOptions);
  input.addEventListener("focus", filterOptions);

  // prevent premature blurring of input field when clicking an option
  ul.addEventListener("mousedown", (event) => event.preventDefault());
}
