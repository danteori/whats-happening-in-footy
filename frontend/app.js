// Ask the Rust backend for its greeting and show it on the page.
const status = document.querySelector("#backend-status");

try {
  const response = await fetch("/api/hello");
  if (!response.ok) throw new Error(`HTTP ${response.status}`);
  const { message } = await response.json();
  status.textContent = message;
} catch (err) {
  status.textContent = `Couldn't reach the backend (${err.message}).`;
  status.classList.add("error");
}
