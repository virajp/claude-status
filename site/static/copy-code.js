/**
 * Copy buttons for every code block.
 *
 * The button is BUILT HERE and never written into the HTML. With scripting off
 * — or on a page served from a non-secure origin, where the async clipboard is
 * not available — no button is created at all, so the site never shows a
 * control that cannot do anything. That is the same property
 * `the_generator_page_reads_as_documentation_without_its_script` protects on
 * the one other scripted page: the documentation is complete without the
 * script, and the script only ever adds.
 *
 * This is the SECOND of two paths under `site/` allowed to load a script, and
 * `tests/site.rs::only_allowlisted_paths_under_site_may_carry_a_script` names
 * both. It is loaded from `base.html` because a copy button belongs on every
 * page's code blocks, and there is no other shell to hang it from.
 *
 * `<pre>` scrolls horizontally (`overflow-x: auto`), so the button cannot be a
 * child of it — it would scroll out of the corner it is pinned to along with
 * the code. Each block is wrapped in a positioned `.code-block` instead, and
 * the button is a sibling of the `<pre>` rather than a descendant.
 */

/** How long the button stays in its copied state before reverting. */
const COPIED_MS = 2000;

/**
 * The clipboard write is the whole feature. Without it there is nothing to
 * offer, so bail before touching the DOM rather than injecting buttons that
 * would throw on click.
 */
if (navigator.clipboard?.writeText) {
  for (const pre of document.querySelectorAll("pre")) {
    addCopyButton(pre);
  }
}

function addCopyButton(pre) {
  const wrapper = document.createElement("div");
  wrapper.className = "code-block";
  pre.replaceWith(wrapper);
  wrapper.append(pre);

  const button = document.createElement("button");
  button.type = "button";
  button.className = "code-copy";

  // The icon is decorative; the accessible name comes from the text beside it,
  // which is visually hidden rather than absent so the control is never an
  // unlabelled glyph.
  const icon = document.createElement("span");
  icon.className = "code-copy-icon";
  icon.setAttribute("aria-hidden", "true");
  icon.textContent = "⧉";

  const name = document.createElement("span");
  name.className = "visually-hidden";
  const lang = pre.querySelector("code")?.dataset.lang;
  name.textContent = lang && lang !== "plain"
    ? `Copy ${lang} code`
    : "Copy code";

  button.append(icon, name);

  // Announced rather than shown: the icon swap is the sighted feedback, and a
  // live region is what carries the same news to a screen reader. It is
  // OUTSIDE the button so updating it cannot disturb the button's own name.
  const status = document.createElement("span");
  status.className = "visually-hidden";
  status.setAttribute("role", "status");

  let revert;
  button.addEventListener("click", async () => {
    // Cleared first so a repeated outcome is a CHANGE to the live region and
    // gets announced again. Writing the same string over itself is silent.
    status.textContent = "";
    try {
      await navigator.clipboard.writeText(pre.textContent);
    }
    catch {
      // A denied permission is the user's answer, not an error to shout about.
      status.textContent = "Copy failed";
      return;
    }
    button.classList.add("is-copied");
    icon.textContent = "✓";
    status.textContent = "Copied";
    clearTimeout(revert);
    revert = setTimeout(() => {
      button.classList.remove("is-copied");
      icon.textContent = "⧉";
      status.textContent = "";
    }, COPIED_MS);
  });

  wrapper.append(button, status);
}
