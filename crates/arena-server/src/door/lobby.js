// Creates a match through the front door and lists one join link per seat.
// The links carry tickets; nothing here stores or logs them.
"use strict";

const form = document.querySelector(".create");
const button = form.querySelector("button");
const status = document.querySelector(".status");
const result = document.querySelector(".result");
const seats = document.querySelector(".seats");
const expiry = document.querySelector(".expiry");

function say(text, isError = false) {
  status.textContent = text;
  status.classList.toggle("error", isError);
}

function seatName(seat, teamSize) {
  const team = seat % 2 === 0 ? "A" : "B";
  const player = Math.floor(seat / 2) + 1;
  return teamSize === 1 ? `Team ${team}` : `Team ${team} · player ${player}`;
}

async function copy(text, input, copyButton) {
  try {
    await navigator.clipboard.writeText(text);
    copyButton.textContent = "Copied";
    setTimeout(() => {
      copyButton.textContent = "Copy";
    }, 2000);
  } catch {
    // Clipboard access can be refused; leave the link selected instead.
    input.focus();
    input.select();
    say("Copy the selected link with Ctrl+C (or ⌘C).");
  }
}

function show(match) {
  seats.replaceChildren();
  for (const { seat, team, link } of match.seats) {
    const item = document.createElement("li");
    item.className = "seat";
    const name = document.createElement("span");
    name.className = `name team-${team.toLowerCase()}`;
    name.textContent = seat === 0 ? `${seatName(seat, match.team_size)} (you)` : seatName(seat, match.team_size);
    const input = document.createElement("input");
    input.type = "text";
    input.readOnly = true;
    input.value = link;
    input.setAttribute("aria-label", `Join link for ${seatName(seat, match.team_size)}`);
    input.addEventListener("focus", () => input.select());
    const copyButton = document.createElement("button");
    copyButton.type = "button";
    copyButton.textContent = "Copy";
    copyButton.addEventListener("click", () => copy(link, input, copyButton));
    const open = document.createElement("a");
    open.href = link;
    open.target = "_blank";
    open.rel = "noopener noreferrer";
    open.textContent = "Open";
    item.append(name, input, copyButton, open);
    seats.append(item);
  }
  const until = new Date(match.expires * 1000);
  expiry.textContent = `at ${until.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}`;
  result.hidden = false;
}

form.addEventListener("submit", async (event) => {
  event.preventDefault();
  const teamSize = Number(new FormData(form).get("team_size"));
  button.disabled = true;
  say("Creating the match…");
  try {
    const response = await fetch("../api/matches", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ team_size: teamSize }),
      cache: "no-store",
      credentials: "omit",
    });
    if (!response.ok) {
      const reason = (await response.text()).trim();
      say(reason || `The server answered ${response.status}.`, true);
      return;
    }
    show(await response.json());
    say(`Match created (${teamSize}v${teamSize}).`);
  } catch {
    say("Could not reach the server. Check your connection and try again.", true);
  } finally {
    button.disabled = false;
  }
});
