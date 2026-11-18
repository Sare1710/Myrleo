(() => {
  const forms = document.querySelectorAll(".search-form");

  for (const form of forms) {
    const input = form.querySelector('input[name="q"]');
    const panel = form.querySelector(".suggestions");
    if (!input || !panel) continue;

    let requestNumber = 0;
    let activeIndex = -1;

    const close = () => {
      panel.hidden = true;
      panel.replaceChildren();
      activeIndex = -1;
    };

    const activate = (index) => {
      const buttons = [...panel.querySelectorAll(".suggestion")];
      buttons.forEach((button) => button.classList.remove("suggestion-active"));
      if (buttons[index]) {
        activeIndex = index;
        buttons[index].classList.add("suggestion-active");
        input.value = buttons[index].dataset.term;
      }
    };

    input.addEventListener("input", async () => {
      const words = input.value.trim().split(/\s+/);
      const prefix = words.at(-1) || "";
      if (prefix.length < 2) {
        close();
        return;
      }

      const currentRequest = ++requestNumber;
      try {
        const response = await fetch(`/api/suggest?q=${encodeURIComponent(prefix)}&limit=7`);
        if (!response.ok || currentRequest !== requestNumber) return;
        const suggestions = await response.json();
        panel.replaceChildren();

        for (const suggestion of suggestions) {
          const button = document.createElement("button");
          button.type = "button";
          button.className = "suggestion";
          button.dataset.term = suggestion.term;

          const label = document.createElement("span");
          label.textContent = suggestion.term;
          const frequency = document.createElement("small");
          frequency.textContent = `${suggestion.frequency} apariții`;
          button.append(label, frequency);

          button.addEventListener("click", () => {
            words[words.length - 1] = suggestion.term;
            input.value = words.join(" ");
            close();
            input.focus();
          });
          panel.append(button);
        }
        panel.hidden = suggestions.length === 0;
        activeIndex = -1;
      } catch {
        close();
      }
    });

    input.addEventListener("keydown", (event) => {
      const count = panel.querySelectorAll(".suggestion").length;
      if (panel.hidden || count === 0) return;
      if (event.key === "ArrowDown") {
        event.preventDefault();
        activate((activeIndex + 1) % count);
      } else if (event.key === "ArrowUp") {
        event.preventDefault();
        activate((activeIndex - 1 + count) % count);
      } else if (event.key === "Escape") {
        close();
      }
    });

    document.addEventListener("click", (event) => {
      if (!form.contains(event.target)) close();
    });
  }
})();
