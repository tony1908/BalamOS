export const CIRCLE_AGENT_WALLET_SKILL_NAME = "Circle Agent Wallets";

export const CIRCLE_AGENT_WALLET_SKILL_INSTRUCTION = `For container-backed Orbit harnesses only, use Circle Agent Wallets in an ARC-TESTNET-first workflow. The Circle wallet CLI is the \`@circle-fin/cli\` package and its executable is \`circle\`; it is not CircleCI and must not be invoked as \`circleci\`. Do not reuse this container setup for host agents such as Antigravity; host agents do not receive the container CLI, /config home, or /workspace mounts.

- Before using the CLI, run command -v circle and circle --version. If either check fails, stop and report that the CLI image or PATH is not ready.
- Initial login is manual in the private workspace desktop terminal as user abc: verify the existing HOME equals /config with printenv HOME, stop if it differs, then run circle wallet login email --testnet. Do not automate login or terms acceptance.
- Inspect agent wallets with circle wallet list --chain ARC-TESTNET --type agent --output json.
- Inspect a known address with circle wallet balance --address ADDR --chain ARC-TESTNET --output json.
- Do not spend automatically. For a transfer explicitly authorized by the user, estimate it first: circle wallet transfer RECIPIENT --amount AMOUNT --address SOURCE --chain ARC-TESTNET --estimate. Omit --estimate only after the user explicitly authorizes that exact transfer.
- The documented status command is circle wallet status --type agent; do not add an undocumented testnet flag.

Keep OTPs, email credentials, session credentials, and tokens out of prompts, memory, project files, SQLite, and frontend state. Login and terms acceptance happen manually in the private workspace terminal. Sessions live in the container's private /config home, not the host project bind mount.

Circle spending policies are mainnet-only. This guidance is not enforced financial security. Fund or transfer only when explicitly authorized by the user. Disabling this skill does not revoke CLI or session access; use the CLI's own logout/session controls. Reset or delete removes local session state but does not remove an onchain wallet or its funds.

Do not automate login or terms acceptance. Fund or transfer only when explicitly authorized by the user.`;
