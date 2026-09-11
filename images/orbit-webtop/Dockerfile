FROM lscr.io/linuxserver/webtop:ubuntu-xfce@sha256:1bd141d5d7aaf3e98e47b7d9665f50657d1628617b4ef47bc3bbd43d726fd77e

ARG CODEX_VERSION=0.149.1
ARG OPENCODE_VERSION=1.18.25
ARG CIRCLE_CLI_VERSION=1.0.0

RUN apt-get update \
    && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
        ca-certificates \
        curl \
        gnupg \
        jq \
        nodejs \
        scrot \
        sudo \
        xdotool \
    && node -e 'const [major,minor,patch]=process.versions.node.split(".").map(Number); if (major < 20 || (major === 20 && (minor < 18 || (minor === 18 && patch < 2)))) process.exit(1)' \
    && npm install --global "@openai/codex@${CODEX_VERSION}" "opencode-ai@${OPENCODE_VERSION}" "@circle-fin/cli@${CIRCLE_CLI_VERSION}" "@anthropic-ai/claude-code" "@google/gemini-cli" \
    && npm cache clean --force \
    && rm -rf /var/lib/apt/lists/*

RUN install -d -m 0755 /etc/apt/keyrings \
    && curl -fsSL https://packages.mozilla.org/apt/repo-signing-key.gpg -o /etc/apt/keyrings/packages.mozilla.org.asc \
    && echo "deb [signed-by=/etc/apt/keyrings/packages.mozilla.org.asc] https://packages.mozilla.org/apt mozilla main" > /etc/apt/sources.list.d/mozilla.list \
    && printf "Package: *\nPin: origin packages.mozilla.org\nPin-Priority: 1000\n" > /etc/apt/preferences.d/mozilla \
    && apt-get update \
    && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends firefox \
    && rm -rf /var/lib/apt/lists/*

# Pin a fixed, roomy desktop resolution so Selkies does not shrink the OS to the
# small preview iframe (agent screenshots + click coordinates stay stable).
ENV SELKIES_IS_MANUAL_RESOLUTION_MODE=true \
    SELKIES_MANUAL_WIDTH=1600 \
    SELKIES_MANUAL_HEIGHT=900 \
    BROWSER=firefox

COPY images/orbit-webtop/orbitctl /usr/local/bin/orbitctl
COPY images/orbit-webtop/orbit-install-apps /usr/local/bin/orbit-install-apps
COPY images/orbit-webtop/skills/orbit-desktop-control/SKILL.md /opt/orbit/skills/orbit-desktop-control/SKILL.md
COPY images/orbit-webtop/opencode/AGENTS.md /opt/orbit/opencode/AGENTS.md
COPY images/orbit-webtop/opencode/opencode.json /opt/orbit/opencode/opencode.json
COPY images/orbit-webtop/20-orbit-codex /custom-cont-init.d/20-orbit-codex
COPY packages/hedera-readonly/package.json /opt/balamos/hedera-src/package.json
COPY packages/hedera-readonly/src/ /opt/balamos/hedera-src/src/
COPY packages/balamos-graph/package.json /opt/balamos/graph-src/package.json
COPY packages/balamos-graph/src/ /opt/balamos/graph-src/src/
COPY packages/balamos-x402/package.json /opt/balamos/x402-src/package.json
COPY packages/balamos-x402/src/ /opt/balamos/x402-src/src/
COPY packages/balamos-graph-mcp/package.json /opt/balamos/graph-mcp-src/package.json
COPY packages/balamos-graph-mcp/src/ /opt/balamos/graph-mcp-src/src/
RUN chmod 0755 /usr/local/bin/orbitctl \
    /usr/local/bin/orbit-install-apps \
    /custom-cont-init.d/20-orbit-codex \
    && printf 'abc ALL=(ALL) NOPASSWD:ALL\n' > /etc/sudoers.d/abc \
    && chmod 0440 /etc/sudoers.d/abc \
    && visudo -cf /etc/sudoers.d/abc \
    && codex --version | grep -F "codex-cli ${CODEX_VERSION}" \
    && opencode --version \
    && circle --version | grep -F "${CIRCLE_CLI_VERSION}" \
    && claude --version \
    && gemini --version \
    && firefox --version
