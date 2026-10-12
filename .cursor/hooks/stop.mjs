process.stdout.write(
  JSON.stringify({
    continue: true,
    agentMessage: [
      "Mote session checklist:",
      "1) docs-obsidian — vault notes updated if code changed?",
      "2) security — no secrets committed; capabilities still minimal?",
      "3) smoke — overlay / chat / tray still make sense?",
    ].join(" "),
  }),
);
