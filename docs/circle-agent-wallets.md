# Circle Agent Wallets

## Architecture and Usage

Circle Agent Wallets is an optional persisted skill in the existing workspace Skills panel. Choose **Add Circle Agent Wallets** to store the prefilled instructions, then enable or disable it like any other skill. Enabled skills are rendered into the managed section of the workspace `AGENTS.md` by the daemon and are available to the selected agent runtime. The UI does not call Circle APIs or hold wallet state.

Circle guidance is scoped to container-backed Orbit harnesses. Antigravity runs on the host and must not reuse the container CLI, `/config` home, or `/workspace` assumptions. The selected Ubuntu webtop image includes a pinned `@circle-fin/cli`; the Alpine Dockerfile is experimental.

Build context is currently blocked: only `skills/orbit-desktop-control/SKILL.md` and `opencode/AGENTS.md` are missing from the `images/orbit-webtop` build context; `orbitctl`, `orbit-install-apps`, and `20-orbit-codex` exist. Do not rerun Docker builds until those assets are restored. When available, the exact commands are `docker build -f images/orbit-webtop/Dockerfile -t orbit-webtop:0.4.0 images/orbit-webtop` and `docker build -f images/orbit-webtop/Dockerfile.alpine -t orbit-webtop:alpine images/orbit-webtop`.

Official references: [Circle documentation](https://developers.circle.com/llms.txt), [Agent Wallets](https://developers.circle.com/agent-stack/agent-wallets), [Agent Wallets quickstart](https://developers.circle.com/agent-stack/agent-wallets/quickstart), and [Circle CLI command reference](https://developers.circle.com/agent-stack/circle-cli/command-reference).

Use ARC-TESTNET first. In the private desktop terminal, as user `abc`, verify the existing `HOME` is `/config` with `printenv HOME`; stop if it differs, then run `circle wallet login email --testnet`. Never put OTPs, email/session credentials, or tokens in prompts, memory, project files, SQLite, or frontend state. Do not automate login or terms acceptance.

## Sessions and Limits

The runtime mounts a named Docker volume at `/config` and the host project separately at `/workspace`; Circle CLI sessions therefore live in the container's private `/config` home, not the host bind-mounted project. Stopping/restarting preserves that named volume. Reset and delete remove local session state by removing the volume. They do not remove an onchain wallet or its funds. Rebuilds affect the image, not an existing volume. Do not use destructive reset or delete as logout; use Circle's logout/session controls.

Circle spending policies are mainnet-only, and these instructions are guidance rather than enforced financial security. No automatic spending occurs. A transfer is allowed only after explicit user authorization of that exact transfer, with an estimate first. Do not automate login or terms acceptance. Fund or transfer only when explicitly authorized by the user.

Skill changes sync only when a harness is newly spawned. The Skills panel displays a restart/new-session notice; restart the agent or start a new session after enabling or editing the skill. The daemon embeds the installer script and must be restarted/deployed before that runtime repair is used by future lifecycle operations; editing the built-in module does not update an already-running Demos container or claim that its persisted skill changed.

The daemon re-applies the embedded installer after healthy provisioning, reset, missing-container recovery, and normal or already-running starts. This is a best-effort workspace repair: Observe does not install over the network, and an online Circle CLI install failure is logged without making the workspace unusable. It does not mount, delete, or change user data and does not restart Demos containers.

## Follow-on Work

Future work can expose structured wallet status in the UI using the documented status command and add an explicit, separately designed integration for enforced spending controls. Neither is implemented here.
