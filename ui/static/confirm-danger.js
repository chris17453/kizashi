// Confirms every destructive form submission before it fires (ADR-0061) -- every ".btn-danger"
// button across the Console UI (revoke, remove, disable) previously submitted immediately on
// click with zero confirmation, a real safety gap for a one-click-permanent action. Attached at
// the document level (not per-button) so any current or future ".btn-danger" submit button is
// covered automatically, no per-page wiring required.
document.addEventListener("submit", function (event) {
  var submitter = event.submitter;
  if (!submitter || !submitter.classList.contains("btn-danger")) {
    return;
  }
  var label = (submitter.textContent || "this action").trim();
  var confirmed = window.confirm(
    "Are you sure you want to " + label.toLowerCase() + "? This cannot be undone."
  );
  if (!confirmed) {
    event.preventDefault();
  }
});
